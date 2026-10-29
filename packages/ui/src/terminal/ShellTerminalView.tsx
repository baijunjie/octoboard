import { Spinner, Surface } from "@heroui/react";
import { Unplug } from "lucide-react";
import React, { useEffect, useImperativeHandle, useRef, useState } from "react";

import "@xterm/xterm/css/xterm.css";
import { Button } from "../components/Button";
import type { ViewedFile } from "../browser/TerminalFileViewer";
import { EmptyPanel } from "../components/EmptyPanel";
import { reportExternalLinkFailure } from "../components/ExternalLink";
import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../components/StatusAnnouncer";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { useLatest } from "../components/useLatest";
import { useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import type { ShellTerminal } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { useOctoboardTheme } from "../theme";
import { openPathTarget } from "./openPathTarget";
import { programCopyHandler } from "./osc52";
import { TerminalController, type TermStatus } from "./TerminalController";
import { TerminalEditMenu } from "./TerminalEditMenu";
import { TerminalLinkTooltip } from "./TerminalLinkTooltip";
import { MAX_AUTO_RECONNECT_ATTEMPTS, reconnectDelay, STABLE_CONNECTION_MS } from "./reconnectPolicy";
import type { HoveredTerminalLink } from "./terminalLinks";
import { XTERM_THEMES } from "./xtermThemes";

export interface ShellTerminalViewHandle {
  /** Puts keyboard focus on the terminal, or on the cover's Reconnect while it offers one, and says
   * whether focus landed in the view: not once the view has gone, nor while its tab is hidden. */
  focus: () => boolean;
}

/**
 * One shell terminal of the Terminal panel: its own `TerminalController`, attached to the
 * terminal's socket for as long as the tab exists, shown or not, so a hidden tab keeps its screen
 * and its connection. It draws with xterm's DOM renderer, never on the GPU (see where the
 * controller is built).
 *
 * It is a shell's view, not a session's: there is no
 * saved output to load (no `detach` with one), as the daemon keeps the terminal's output for as
 * long as the terminal exists. What to do about a dropped socket is decided here from the record:
 * a running shell is reattached, which replays its output; an exited one is not, and its final
 * screen stays on until the tab is closed.
 *
 * The element xterm mounts in carries `data-shell-terminal`, which keeps Escape for the shell
 * inside a floating pane (`keepsEscape` in `layout/usePaneToggles.ts`); the shown one's input
 * carries `data-region-landing`, where F6 and a maximize put focus in the panel, or its cover's
 * Reconnect while the cover offers one, since xterm keeps Tab and a keyboard user would never
 * reach the button from the input.
 */
export function ShellTerminalView({
  terminal,
  shown,
  resizing = false,
  onViewFile,
  onClose,
  ref,
}: {
  terminal: ShellTerminal;
  /** Whether its tab is the one shown. */
  shown: boolean;
  /** The panel is easing in size: the view keeps its size until the ease ends, then fits once, as
   * the agent terminal does (`TerminalPane`), since each refit makes the shell redraw its prompt. */
  resizing?: boolean;
  /** Shows a project file a path in the output named, in the file viewer, which hands focus back
   * to this terminal through `returnFocus` once it closes. */
  onViewFile?: (file: ViewedFile, returnFocus?: () => boolean) => void;
  /** Closes the terminal, from the notice an exited shell leaves. */
  onClose: () => void;
  ref?: React.Ref<ShellTerminalViewHandle>;
}): React.ReactElement {
  const t = useT();
  const { terminalUrl, terminalReconnectDelay, request, toastError } = useDaemon();
  const { links, clipboard } = usePlatform();
  const { resolved: colorTheme } = useOctoboardTheme();
  const rootRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const coverRef = useRef<HTMLDivElement>(null);
  const controllerRef = useRef<TerminalController | undefined>(undefined);
  const setLinkRef = useRef<((link: HoveredTerminalLink | undefined) => void) | undefined>(undefined);
  // Attached as the view mounts, so it starts connecting: a `closed` start would read as a dropped
  // connection for the first render.
  const [status, setStatus] = useState<TermStatus>("connecting");
  const attemptsRef = useRef(0);
  const stableTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const appliedColorThemeRef = useRef(colorTheme);
  const resizingRef = useLatest(resizing);
  const latest = useLatest({ request, toastError, onViewFile, project: terminal.project_id, t });
  const running = terminal.state === "running";
  const connectionState = useDaemonStore((s) => s.connectionState);
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);

  // Reached through the handle and from the file viewer's close, which can come after this view has gone.
  const focus = () => {
    const root = rootRef.current;
    if (!root?.isConnected) return false;
    const button = coverRef.current?.querySelector("button");
    if (button) button.focus();
    else controllerRef.current?.focus();
    return root.contains(document.activeElement);
  };
  // The cover's Reconnect is where focus lands in a dropped terminal, and it goes without a handler
  // of its own when the connection comes back, the control connection drops or the shell exits.
  const held = useFocusHandoff(() => controllerRef.current?.focus());

  const attach = (userInitiated: boolean) => controllerRef.current?.attach(terminal.id, terminalUrl(terminal.id), userInitiated);

  useEffect(() => {
    const controller = new TerminalController(
      {
        onStatusChange: (next) => {
          clearTimeout(stableTimerRef.current);
          if (next === "open") stableTimerRef.current = setTimeout(() => (attemptsRef.current = 0), STABLE_CONNECTION_MS);
          setStatus(next);
        },
        openLink: links && ((url) => links.open(url).catch((cause: unknown) => reportExternalLinkFailure(latest.current.t, url, cause))),
        onLinkHover: (link) => setLinkRef.current?.(link),
        copyToClipboard: programCopyHandler(clipboard, () => latest.current),
        pathLinks: {
          probe: async (paths) => {
            const reply = await latest.current.request({ type: "probe_path_links", project: latest.current.project, paths });
            return reply.type === "path_links" ? reply.links.map(({ kind, view, viewer_only }) => ({ kind, view, viewerOnly: viewer_only })) : [];
          },
          open: (target, onHost) => {
            const onViewFile = latest.current.onViewFile;
            return openPathTarget({
              target,
              onHost,
              project: latest.current.project,
              request: latest.current.request,
              viewFile: onViewFile && ((file) => onViewFile(file, focus)),
              toastError: latest.current.toastError,
            });
          },
        },
      },
      appliedColorThemeRef.current,
      // A WebGL context per tab would not fit the page's budget of them, and WebKit counts one an
      // addon has let go of until it is garbage collected, then takes the context away from the
      // terminal that drew least recently, which can be the agent terminal's. So a shell keeps the
      // DOM renderer, which costs nothing while its tab is hidden. A hidden tab cannot be fitted, so
      // the shell is told no size until the tab has been shown.
      { gpu: false, fittedSizesOnly: true },
    );
    controllerRef.current = controller;
    if (containerRef.current) controller.mount(containerRef.current);
    controller.attach(terminal.id, terminalUrl(terminal.id), false);
    const resizeObserver = new ResizeObserver(() => {
      // The observer does not fire again after the last size of the ease; the effect below fits.
      if (resizingRef.current) return;
      controller.fit();
      controller.syncSize();
    });
    if (containerRef.current) resizeObserver.observe(containerRef.current);
    return () => {
      clearTimeout(stableTimerRef.current);
      resizeObserver.disconnect();
      controller.dispose();
    };
    // One controller and one attachment for the life of the tab, which is keyed by the terminal.
  }, []);

  // The observer skipped the sizes of the ease, so the view fits once it has ended. A hidden tab
  // has no size to fit, and fits when it is shown.
  const wasResizingRef = useRef(false);
  useEffect(() => {
    const ended = wasResizingRef.current && !resizing;
    wasResizingRef.current = resizing;
    if (!ended || !shown) return;
    controllerRef.current?.fit();
    controllerRef.current?.syncSize();
  }, [resizing]);

  const inputLabel = t("terminal.input");
  useEffect(() => {
    controllerRef.current?.setInputLabel(inputLabel);
  }, [inputLabel]);

  useEffect(() => {
    const controller = controllerRef.current;
    if (!controller || colorTheme === appliedColorThemeRef.current) return;
    appliedColorThemeRef.current = colorTheme;
    controller.setColorTheme(colorTheme);
  }, [colorTheme]);

  // A running shell's socket that closed is a dropped connection: the daemon closes it on its own
  // only for a shell that has exited, which the record then says. Never user-initiated, so a
  // background heal does not pull focus.
  useEffect(() => {
    if (!running || (status !== "closed" && status !== "not_running")) return;
    if (attemptsRef.current >= MAX_AUTO_RECONNECT_ATTEMPTS) return;
    const delay = reconnectDelay(attemptsRef.current, terminalReconnectDelay);
    const timer = setTimeout(() => {
      attemptsRef.current += 1;
      attach(false);
    }, delay);
    return () => clearTimeout(timer);
  }, [status, running, terminalReconnectDelay]);

  // A fresh snapshot (the control connection came back) is a chance to recover a terminal whose
  // automatic attempts were spent.
  useEffect(() => {
    if (!running || (status !== "closed" && status !== "not_running") || attemptsRef.current < MAX_AUTO_RECONNECT_ATTEMPTS) return;
    attemptsRef.current = 0;
    attach(false);
    // Keyed on the epoch alone; the status is the reconnect effect's business.
  }, [snapshotEpoch]);

  // What is wrong with a running shell's connection once an attempt has failed: `disconnected`
  // once the automatic attempts are spent, `reconnecting` until then. A first connect that has not
  // failed yet is no trouble (typing meanwhile is queued).
  const [trouble, setTrouble] = useState<"reconnecting" | "disconnected">();
  useEffect(() => {
    const failed = status === "closed" || status === "not_running";
    const attempts = attemptsRef.current;
    if (!running || !(failed || (status === "connecting" && attempts > 0))) setTrouble(undefined);
    else setTrouble(failed && attempts >= MAX_AUTO_RECONNECT_ATTEMPTS ? "disconnected" : "reconnecting");
  }, [status, running]);
  const canReconnect = trouble === "disconnected" && connectionState === "open";
  // Where F6 lands in the shown tab: the cover's Reconnect while it offers one, else the input.
  // xterm creates its input in `mount`, so the attribute goes on it here rather than in markup.
  useEffect(() => {
    const button = coverRef.current?.querySelector("button");
    controllerRef.current?.term.textarea?.toggleAttribute("data-region-landing", shown && !button);
    button?.toggleAttribute("data-region-landing", shown);
  }, [shown, trouble, canReconnect]);
  // Typing into a terminal that is not connected goes nowhere, so when Reconnect appears while focus
  // is in the view (on the terminal), it takes focus. Focus elsewhere in the window is left alone: a
  // background failure must not pull it.
  useEffect(() => {
    if (!canReconnect || !rootRef.current?.contains(document.activeElement)) return;
    coverRef.current?.querySelector("button")?.focus();
  }, [canReconnect]);
  const reconnectNow = () => {
    attemptsRef.current = 0;
    attach(true);
  };

  useImperativeHandle(ref, () => ({ focus }), []);

  return (
    <div
      ref={rootRef}
      id={`shell-terminal-${terminal.id}`}
      className={`absolute inset-0 flex-col ${shown ? "flex" : "hidden"}`}
      style={{ "--terminal-background": XTERM_THEMES[colorTheme].background, backgroundColor: "var(--terminal-background)" } as React.CSSProperties}
    >
      {/* Padding on a wrapper, not on the element xterm is mounted in, which the fit addon sizes the
          terminal from; isolation keeps xterm's link canvas below the overlays. */}
      <div className="isolate min-h-0 flex-1 ps-(--pane-inset) pe-2 py-2">
        {/* xterm lays its cells out left to right whatever the page's direction. */}
        <div dir="ltr" data-shell-terminal className="h-full overflow-hidden" ref={containerRef} />
      </div>
      <TerminalEditMenu sessionId={terminal.id} controllerRef={controllerRef} contextTargetRef={containerRef} />
      <TerminalLinkTooltip setLinkRef={setLinkRef} />
      {trouble && (
        <div ref={coverRef} {...held} className="absolute inset-0 flex flex-col items-center justify-center-safe text-sm text-app-muted" style={{ backgroundColor: "var(--terminal-background)" }}>
          {trouble === "disconnected" ? (
            <EmptyPanel icon={Unplug} message={t("terminal.disconnected")} action={canReconnect ? { label: t("terminal.reconnect"), onPress: reconnectNow } : undefined} />
          ) : (
            <div aria-hidden="true" className="flex flex-col items-center gap-3">
              <Spinner size="md" color="current" aria-hidden="true" />
              {t("terminal.reconnecting")}
            </div>
          )}
        </div>
      )}
      {/* Spoken from status regions mounted empty with each showing of the tab and written after the
          first delay (`StatusAnnouncer`): the tab's root is `display: none` while hidden, so a region kept
          there would write its text in the render that shows the tab, together with the region. The
          delay is the one for a focus move, which pressing a tab or starting a terminal makes into the
          view. The exit sentence is hidden from assistive technology on screen, so nothing is said
          twice, and its button stays reachable. */}
      {shown && <StatusAnnouncer text={trouble ? t(trouble === "disconnected" ? "terminal.disconnected" : "terminal.reconnecting") : undefined} firstWriteDelayMs={AFTER_FOCUS_MOVE_DELAY_MS} />}
      {shown && <StatusAnnouncer text={!running ? t("shellTerminal.exited") : undefined} firstWriteDelayMs={AFTER_FOCUS_MOVE_DELAY_MS} />}
      <div className="pointer-events-none absolute inset-x-0 bottom-4 flex justify-center">
        {shown && !running && (
          <Surface className="floating-surface pointer-events-auto flex items-center gap-3 rounded-lg px-3 py-2 text-sm shadow-lg">
            <span aria-hidden="true" className="text-app-muted">
              {t("shellTerminal.exited")}
            </span>
            <Button size="sm" variant="secondary" preventFocusOnPress onPress={onClose}>
              {t("shellTerminal.close")}
            </Button>
          </Surface>
        )}
      </div>
    </div>
  );
}
