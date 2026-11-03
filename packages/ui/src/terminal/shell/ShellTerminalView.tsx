import React, { useCallback, useEffect, useImperativeHandle, useRef } from "react";

import "@xterm/xterm/css/xterm.css";
import type { ViewedFile } from "../../viewer/viewedFile";
import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../../components/announce/StatusAnnouncer";
import { useFocusHandoff } from "../../components/focus/useFocusHandoff";
import { useT } from "../../i18n/react";
import type { ShellTerminal } from "../../protocol";
import { useDaemon } from "../../store";
import { TerminalController, type TerminalControllerOptions } from "../TerminalController";
import { TerminalCover } from "../TerminalCover";
import { TerminalEditMenu } from "../TerminalEditMenu";
import { TerminalNoticeBar, TerminalNoticeSlot } from "../TerminalNoticeBar";
import { TerminalSurface } from "../TerminalSurface";
import { useTerminalController } from "../useTerminalController";
import { useTerminalReconnect } from "../useTerminalReconnect";
import type { HoveredTerminalLink } from "../links/terminalLinks";

export interface ShellTerminalViewHandle {
  /** Puts keyboard focus on the terminal, or on the cover's Reconnect while it offers one, and says
   * whether focus landed in the view: not once the view has gone, nor while its tab is hidden. */
  focus: () => boolean;
}

// A WebGL context per tab would not fit the page's budget of them, and WebKit counts one an addon
// has let go of until it is garbage collected, then takes the context away from the terminal that
// drew least recently, which can be the agent terminal's. So a shell keeps the DOM renderer, which
// costs nothing while its tab is hidden. A hidden tab cannot be fitted, so the shell is told no size
// until the tab has been shown.
const SHELL_CONTROLLER_OPTIONS: TerminalControllerOptions = { gpu: false, fittedSizesOnly: true };

/**
 * One shell terminal of the Terminal panel: its own `TerminalController`, attached to the
 * terminal's socket for as long as the tab exists, shown or not, so a hidden tab keeps its screen
 * and its connection. It draws with xterm's DOM renderer, never on the GPU (see
 * `SHELL_CONTROLLER_OPTIONS`).
 *
 * It is a shell's view, not a session's: there is no saved output to load (no `detach` with one), as
 * the daemon keeps the terminal's output for as long as the terminal exists. What to do about a
 * dropped socket is decided here from the record: a running shell is reattached, which replays its
 * output; an exited one is not, and its final screen stays on until the tab is closed.
 *
 * The element xterm mounts in carries `data-shell-terminal`, which keeps Escape for the shell
 * inside a floating pane (`keepsEscape` in `components/focus/keepsEscape.ts`); the shown one's input
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
  const { terminalUrl, terminalReconnectDelay } = useDaemon();
  const rootRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const coverRef = useRef<HTMLDivElement>(null);
  const controllerRef = useRef<TerminalController | undefined>(undefined);
  const setLinkRef = useRef<((link: HoveredTerminalLink | undefined) => void) | undefined>(undefined);
  const running = terminal.state === "running";

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

  const attach = useCallback(
    (userInitiated: boolean) => controllerRef.current?.attach(terminal.id, terminalUrl(terminal.id), userInitiated),
    [terminal.id, terminalUrl],
  );
  // A running shell's socket that closed is a dropped connection: the daemon closes it on its own
  // only for a shell that has exited, which the record then says. Attached as the view mounts, so it
  // starts connecting: a `closed` start would read as a dropped connection for the first render.
  const { onStatusChange, trouble, canReconnect, reconnectNow } = useTerminalReconnect({
    controllerRef,
    initialStatus: "connecting",
    targetId: terminal.id,
    live: running,
    attach,
    terminalReconnectDelay,
  });
  useTerminalController({
    controllerRef,
    containerRef,
    setLinkRef,
    resizing,
    shown,
    onStatusChange,
    project: terminal.project_id,
    viewFile: onViewFile && ((file) => onViewFile(file, focus)),
    options: SHELL_CONTROLLER_OPTIONS,
  });
  // One attachment for the life of the tab, which is keyed by the terminal.
  useEffect(() => {
    attach(false);
  }, []);

  // Where F6 lands in the shown tab: the cover's Reconnect while it offers one, else the input.
  // xterm creates its input in `mount`, so the attribute goes on it here rather than in markup.
  useEffect(() => {
    const button = coverRef.current?.querySelector("button");
    controllerRef.current?.term.textarea?.toggleAttribute("data-region-landing", shown && !button);
    button?.toggleAttribute("data-region-landing", shown);
  }, [shown, trouble, canReconnect]);

  useImperativeHandle(ref, () => ({ focus }), []);
  const troubleText = t(trouble === "disconnected" ? "terminal.disconnected" : "terminal.reconnecting");

  return (
    <TerminalSurface
      ref={rootRef}
      id={`shell-terminal-${terminal.id}`}
      className={`absolute inset-0 flex-col ${shown ? "flex" : "hidden"}`}
      containerRef={containerRef}
      setLinkRef={setLinkRef}
      padding="ps-(--pane-inset) pe-2 py-2"
      mountMarker="data-shell-terminal"
      editMenu={<TerminalEditMenu sessionId={terminal.id} controllerRef={controllerRef} contextTargetRef={containerRef} />}
    >
      {trouble && (
        <TerminalCover
          ref={coverRef}
          rootRef={rootRef}
          held={held}
          text={troubleText}
          spinner
          disconnected={trouble === "disconnected"}
          canReconnect={canReconnect}
          onReconnect={reconnectNow}
        />
      )}
      {/* Spoken from status regions mounted empty with each showing of the tab and written after the
          first delay (`StatusAnnouncer`): the tab's root is `display: none` while hidden, so a region kept
          there would write its text in the render that shows the tab, together with the region. The
          delay is the one for a focus move, which pressing a tab or starting a terminal makes into the
          view. The exit sentence is hidden from assistive technology on screen, so nothing is said
          twice, and its button stays reachable. */}
      {shown && <StatusAnnouncer text={trouble ? troubleText : undefined} firstWriteDelayMs={AFTER_FOCUS_MOVE_DELAY_MS} />}
      {shown && <StatusAnnouncer text={!running ? t("shellTerminal.exited") : undefined} firstWriteDelayMs={AFTER_FOCUS_MOVE_DELAY_MS} />}
      <TerminalNoticeSlot>
        {shown && !running && (
          <TerminalNoticeBar sentence={t("shellTerminal.exited")} action={t("shellTerminal.close")} variant="secondary" onPress={onClose} />
        )}
      </TerminalNoticeSlot>
    </TerminalSurface>
  );
}
