import { Button, Spinner, Surface } from "@heroui/react";
import { SquareTerminal, Unplug } from "lucide-react";
import React, { useCallback, useEffect, useImperativeHandle, useLayoutEffect, useRef, useState } from "react";

import "@xterm/xterm/css/xterm.css";
import { EmptyPanel } from "../components/EmptyPanel";
import { focusTopBar } from "../components/focusTopBar";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { useT } from "../i18n/react";
import { isDormant as isDormantStatus, isLive, type Session } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { useOctoboardTheme } from "../theme";
import { TerminalController, type TermStatus } from "./TerminalController";
import { XTERM_THEMES } from "./xtermThemes";

/** The longest the loading state covers a connected terminal still waiting for its first output. */
const LOADING_LIMIT_MS = 10_000;

interface TerminalPaneProps {
  /** The session whose terminal should be shown, or `undefined` when nothing is selected yet. */
  session?: Session;
  /** Resumes a session; resolves to whether it was accepted. */
  onResume: (sessionId: string) => Promise<boolean>;
  /** A resume of `session` is in flight: its process is being started. */
  resuming?: boolean;
  ref?: React.Ref<TerminalPaneHandle>;
}

export interface TerminalPaneHandle {
  /** Puts keyboard focus on the terminal, or on the cover's Reconnect button while it is offered:
   * the terminal's Tab belongs to the agent, so this is how the keyboard reaches the button. */
  focus: () => void;
}

/** Reconnect backoff after a dropped-but-still-live session's socket closes: doubles each attempt,
 * capped, and stops after `MAX_AUTO_RECONNECT_ATTEMPTS` so a session that keeps failing to connect
 * does not retry forever — the cover's Reconnect covers that case instead. The attempt counter only
 * resets once a connection has *stayed* open for `STABLE_CONNECTION_MS` — resetting it on every
 * `open` would mean a connection that opens and drops again within a few seconds (a backpressure
 * drop recurs roughly that often under sustained output) always sees attempt 0, so the delay would
 * never grow past the base value and the cap would never engage. */
const MAX_AUTO_RECONNECT_ATTEMPTS = 5;
const BASE_RECONNECT_DELAY_MS = 1000;
const MAX_RECONNECT_DELAY_MS = 8000;
const STABLE_CONNECTION_MS = 10000;

/**
 * React boundary around `TerminalController`: one controller instance for the life of the
 * component (not per session — see the controller's own doc comment for why session/socket/status
 * /focus must change together), mounted into a plain `div` it owns directly rather than through
 * React's own DOM diffing, since xterm.js manages that subtree itself.
 */
export function TerminalPane({
  session,
  onResume,
  resuming = false,
  ref,
}: TerminalPaneProps): React.ReactElement {
  const t = useT();
  const { terminalUrl, terminalReconnectDelay } = useDaemon();
  const { resolved: colorTheme } = useOctoboardTheme();
  const containerRef = useRef<HTMLDivElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const coverRef = useRef<HTMLDivElement>(null);
  const controllerRef = useRef<TerminalController | undefined>(undefined);
  const [status, setStatus] = useState<TermStatus>("closed");
  const [painted, setPainted] = useState(false);
  // How the selected session's connection is doing once an attempt has failed (`undefined` while
  // it is healthy or has not failed yet); `reconnecting` until the automatic attempts are spent.
  // Stored with the session it was worked out for, so that the first render after a session switch
  // does not show the previous session's.
  const [troubleOf, setTroubleOf] = useState<{ session: string; state: "reconnecting" | "disconnected" }>();
  const trouble = troubleOf?.session === session?.id ? troubleOf?.state : undefined;
  const reconnectAttemptsRef = useRef(0);
  const stableTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  // What the controller's theme currently is: the construction below reads it for the initial
  // value, and the `colorTheme` effect further down both reads and updates it. One ref rather than
  // two so the two can never claim different things about the same instance.
  const appliedColorThemeRef = useRef(colorTheme);

  const connectionState = useDaemonStore((s) => s.connectionState);
  // While the daemon connection is down the connection banner owns recovery and its Retry; the
  // terminal's own Reconnect is not offered then.
  const canReconnect = trouble === "disconnected" && connectionState === "open";
  // With no session the terminal is covered and inert and cannot take focus, so focus asked for it
  // goes to the top bar wherever it is now, except on `<body>`: `refocusIfLost` asks at mount, and
  // launching the app must not pull focus onto a control.
  const hasSessionRef = useRef(false);
  hasSessionRef.current = session !== undefined;
  const focusTerminal = useCallback(() => {
    if (hasSessionRef.current) controllerRef.current?.focus();
    else if (document.activeElement !== document.body) focusTopBar();
  }, []);
  // The session going away turns the terminal inert, and focus inside it would then fall to
  // `<body>` with no event; hand it to the top bar first.
  const noSession = session === undefined;
  useLayoutEffect(() => {
    if (noSession && rootRef.current?.contains(document.activeElement)) focusTopBar();
  }, [noSession]);
  const held = useFocusHandoff(focusTerminal);

  useImperativeHandle(
    ref,
    () => ({
      focus: () => {
        const button = coverRef.current?.querySelector("button");
        if (button) button.focus();
        else focusTerminal();
      },
    }),
    [focusTerminal],
  );
  // Typing into a terminal that is not connected goes nowhere, so when the Reconnect button appears
  // while focus is in the pane (on the terminal), it takes focus. Focus elsewhere in the window is
  // left alone: a background failure must not pull it.
  useEffect(() => {
    if (!canReconnect || !rootRef.current?.contains(document.activeElement)) return;
    coverRef.current?.querySelector("button")?.focus();
  }, [canReconnect]);
  // The controller is built once; what it calls on input that wakes an archived session must be
  // the latest `onResume`.
  const onResumeRef = useRef(onResume);
  onResumeRef.current = onResume;

  const handleStatusChange = useCallback((s: TermStatus) => {
    if (stableTimerRef.current) {
      clearTimeout(stableTimerRef.current);
      stableTimerRef.current = undefined;
    }
    if (s === "open") {
      stableTimerRef.current = setTimeout(() => {
        reconnectAttemptsRef.current = 0;
      }, STABLE_CONNECTION_MS);
    }
    setStatus(s);
  }, []);

  useEffect(() => {
    const controller = new TerminalController(
      {
        onStatusChange: handleStatusChange,
        onPaintedChange: setPainted,
        onWake: (sessionId) => onResumeRef.current(sessionId),
      },
      appliedColorThemeRef.current,
    );
    controllerRef.current = controller;
    if (containerRef.current) controller.mount(containerRef.current);

    const resizeObserver = new ResizeObserver(() => {
      controller.fit();
      controller.syncSize();
    });
    if (containerRef.current) resizeObserver.observe(containerRef.current);

    return () => {
      if (stableTimerRef.current) clearTimeout(stableTimerRef.current);
      resizeObserver.disconnect();
      controller.dispose();
    };
    // `handleStatusChange` is stable (its own deps are empty); this effect is mount/unmount-only.
  }, []);

  // Names the terminal's input in the UI language; it follows a language change in place, as
  // xterm only names the input once, when it opens.
  const inputLabel = t("terminal.input");
  useEffect(() => {
    controllerRef.current?.setInputLabel(inputLabel);
  }, [inputLabel]);

  // Recolours the running terminal in place rather than recreating it — the instance above is
  // mount-only, per its own comment, and `setColorTheme` is exactly the escape hatch for a change
  // that must still reach it. Skipped when `colorTheme` already matches what the controller has:
  // without this check, the constructor's own initial theme and this effect's first run (which
  // land in the same task as `mount()`'s `term.open()`) would apply the identical colours twice,
  // the second time via `setColorTheme`, which drives a theme-service and renderer refresh — the
  // same class of operation `TerminalController.mount()` defers `fit`/`reset` around because xterm
  // throws from its own scheduled work when they run in that task.
  useEffect(() => {
    const controller = controllerRef.current;
    if (!controller || colorTheme === appliedColorThemeRef.current) return;
    appliedColorThemeRef.current = colorTheme;
    controller.setColorTheme(colorTheme);
  }, [colorTheme]);

  const isDormant = session ? isDormantStatus(session.status) : false;
  // Whether the session's socket should currently be up at all, per the session's own stored
  // status rather than the controller's last-known socket outcome — a reattach can land on
  // `not_running` by losing a race against the session genuinely ending, so only the authoritative
  // `Session` record decides between offering Resume (truly dormant) and Reconnect (still live,
  // just not connected right now).
  const sessionIsLive = session ? isLive(session.status) : false;

  const attachCurrent = useCallback(
    (userInitiated: boolean) => {
      const controller = controllerRef.current;
      if (!controller || !session) return;
      controller.attach(session.id, terminalUrl(session.id), userInitiated);
    },
    // Keyed on the id alone, so only `session.id` may be read in here: anything else would go
    // stale, since a status change does not produce a new callback.
    [session?.id, terminalUrl],
  );

  useEffect(() => {
    reconnectAttemptsRef.current = 0;
  }, [session?.id]);

  useEffect(() => {
    const controller = controllerRef.current;
    if (!controller) return;
    // A dormant session is resumed by the caller before it is ever attached to directly (selecting
    // an interrupted one, or typing to or reopening an archived one) — the daemon has no process to
    // connect the terminal socket to for a session with no running process
    // (`apps/daemon/PROTOCOL.md`), only the output its last one left. Once the resume's
    // `session_upserted` broadcast flips the status, this effect re-runs and attaches for real.
    if (!session || isDormant) {
      // `detach` keeps the screen when it already holds this session's output, and otherwise
      // replaces it with the output the daemon saved for this session.
      // Whatever attempts were spent belonged to the process that is gone.
      reconnectAttemptsRef.current = 0;
      controller.detach({
        keepScreenFor: session?.id,
        savedOutputUrl: session ? terminalUrl(session.id) : undefined,
      });
      // An archived session is not reopened by selecting it, only by typing to it: the first
      // input starts it (`onResume`), and is handed to it once connected.
      if (session?.status === "archived") controller.armWake(session.id);
      else controller.disarmWake();
      return;
    }
    // Only skip when a connection to this exact session is already in flight or open — any other
    // status for this same session id means selecting it (again) should attach for real, not no-op.
    const alreadyAttached =
      controller.currentSessionId === session.id &&
      (controller.currentStatus === "connecting" || controller.currentStatus === "open");
    if (alreadyAttached) return;
    // A broadcast for the session already selected, whose automatic attempts are spent, is not a
    // reason to attach again: that would flip the cover back to reconnecting and pull focus to the
    // terminal on every status update. Only the user's Reconnect (`reconnectNow`) retries then.
    if (controller.currentSessionId === session.id && reconnectAttemptsRef.current >= MAX_AUTO_RECONNECT_ATTEMPTS) return;
    attachCurrent(true);
  }, [session, isDormant, attachCurrent, terminalUrl]);

  // A fresh snapshot (the control connection came back, possibly after an outage longer than the
  // automatic attempts' budget) is a deliberate chance to recover a terminal that gave up: the
  // attach effect above skips ordinary broadcasts once the attempts are spent. Not user-initiated,
  // so it does not pull focus.
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  useEffect(() => {
    const controller = controllerRef.current;
    if (!controller || !session || isDormant || !sessionIsLive) return;
    if (controller.currentSessionId !== session.id) return;
    if (controller.currentStatus !== "closed" && controller.currentStatus !== "not_running") return;
    if (reconnectAttemptsRef.current < MAX_AUTO_RECONNECT_ATTEMPTS) return;
    reconnectAttemptsRef.current = 0;
    attachCurrent(false);
    // Keyed on the epoch alone: the session's own changes are the attach effect's business.
  }, [snapshotEpoch]);

  // Reconnects a session that is still live (per its own `Session` record) after its socket landed
  // on `closed` or `not_running` — the daemon drops a client that stops draining for a few seconds
  // (an occluded window, a large output burst) and documents that the client is expected to
  // reconnect and get the ring buffer replayed, not treat the drop as the session having ended; and
  // a reattach that itself fails to connect (`not_running`) needs the same retry, not a dead end.
  // Never user-initiated: a background heal must not pull focus out of whatever the user is
  // typing into.
  useEffect(() => {
    const controller = controllerRef.current;
    if (!controller || !session) return;
    if (isDormant || !sessionIsLive) return;
    if (status !== "closed" && status !== "not_running") return;
    if (controller.currentSessionId !== session.id) return;
    if (reconnectAttemptsRef.current >= MAX_AUTO_RECONNECT_ATTEMPTS) return;
    const delay =
      terminalReconnectDelay?.(reconnectAttemptsRef.current) ??
      Math.min(BASE_RECONNECT_DELAY_MS * 2 ** reconnectAttemptsRef.current, MAX_RECONNECT_DELAY_MS);
    const timer = setTimeout(() => {
      reconnectAttemptsRef.current += 1;
      attachCurrent(false);
    }, delay);
    return () => clearTimeout(timer);
    // Deliberately keyed on the session's id rather than its record: a status broadcast arriving
    // for this session would otherwise tear the pending timer down and start it over, so under a
    // working agent the reconnect could be postponed indefinitely.
  }, [status, session?.id, isDormant, sessionIsLive, attachCurrent, terminalReconnectDelay]);

  const reconnectNow = useCallback(() => {
    reconnectAttemptsRef.current = 0;
    attachCurrent(true);
  }, [attachCurrent]);

  // Works out what is wrong with the connection: `disconnected` once the attempts are spent.
  // The controller's own status is read rather than `status`: a session switch attaches in an
  // effect above, so the state still holds the previous session's outcome for one render, while the
  // controller already has the new one's. A first connect that has not failed yet is not trouble
  // (typing meanwhile is queued); once an attempt has failed, `connecting` is the reconnect cycle's
  // middle and stays `reconnecting`.
  useEffect(() => {
    const controller = controllerRef.current;
    let next: typeof trouble;
    if (controller && session && !isDormant && sessionIsLive && controller.currentSessionId === session.id) {
      const current = controller.currentStatus;
      const failed = current === "closed" || current === "not_running";
      if (current === "open" || (current === "connecting" && reconnectAttemptsRef.current === 0)) next = undefined;
      else if (failed && reconnectAttemptsRef.current >= MAX_AUTO_RECONNECT_ATTEMPTS) next = "disconnected";
      else next = "reconnecting";
    }
    setTroubleOf(session && next ? { session: session.id, state: next } : undefined);
  }, [status, session?.id, isDormant, sessionIsLive]);

  // What covers the terminal. A connection in trouble comes first, over any old output as well: a
  // terminal that is not connected has nothing live to show, and the disconnected state carries the
  // Reconnect. Otherwise, from selecting a session until its first output is on screen — through a
  // resume starting the process, the socket connecting, and a freshly started agent drawing its
  // first frame — the blank terminal is covered by a loading state. An agent that stays silent does
  // not keep it up forever: once the socket has been open for `LOADING_LIMIT_MS` the bare terminal
  // is shown. The count only runs while the socket is open, so the limit never uncovers a terminal
  // that is not connected (a first connect that hangs, a manual Reconnect under way, a resume still
  // starting the process).
  const awaitingOutput = session !== undefined && !painted && (resuming || !isDormant);
  const [loadingExpired, setLoadingExpired] = useState(false);
  useEffect(() => {
    setLoadingExpired(false);
  }, [awaitingOutput, session?.id]);
  useEffect(() => {
    if (!awaitingOutput || status !== "open") return;
    const timer = setTimeout(() => setLoadingExpired(true), LOADING_LIMIT_MS);
    return () => clearTimeout(timer);
  }, [awaitingOutput, session?.id, status]);
  const loading = awaitingOutput && !loadingExpired;
  const cover: { text: string; spinner: boolean } | undefined = trouble
    ? { text: t(trouble === "disconnected" ? "terminal.disconnected" : "terminal.reconnecting"), spinner: trouble === "reconnecting" }
    : loading
      ? { text: t(resuming ? "terminal.resuming" : "terminal.loading"), spinner: true }
      : undefined;

  return (
    // The background must match the active xterm theme's (`XTERM_THEMES`, read here rather than
    // duplicated as a literal), so the padding around the terminal is not a different colour —
    // a plain Tailwind class cannot do this since the colour now changes at runtime with the
    // theme, not at build time.
    //
    // `data-escape-scope`: one of the origins `usePaneToggles`'s capture-phase Escape listener
    // closes a drawer for — xterm.js holds keyboard focus here most of the time and would otherwise
    // swallow the key before that listener's own bubble-phase alternative ever saw it.
    <div
      ref={rootRef}
      data-escape-scope
      data-region="terminal"
      className="relative flex min-h-0 min-w-0 flex-1 flex-col"
      style={{ backgroundColor: XTERM_THEMES[colorTheme].background }}
    >
      {/* The padding sits on a wrapper, not on the element xterm is mounted in: the fit addon sizes
          the terminal from that element's computed height and width, padding included, so padding
          there gives it rows and columns that the overflow then clips. */}
      <div inert={!session} className="min-h-0 flex-1 px-3 py-2">
        {/* xterm lays its cells out left to right whatever the page's direction. */}
        <div dir="ltr" className="h-full overflow-hidden" ref={containerRef} />
      </div>
      {cover && (
        <div
          ref={coverRef}
          {...held}
          className="absolute inset-0 flex flex-col items-center justify-center text-sm text-muted"
          style={{ backgroundColor: XTERM_THEMES[colorTheme].background }}
        >
          {trouble === "disconnected" ? (
            // The reconnect its button starts moves focus to the terminal, and `held` covers the
            // button vanishing some other way.
            <EmptyPanel
              icon={Unplug}
              message={cover.text}
              action={canReconnect ? { label: t("terminal.reconnect"), onPress: reconnectNow } : undefined}
            />
          ) : (
            <div aria-hidden="true" className="flex flex-col items-center gap-3">
              {cover.spinner && <Spinner size="md" color="current" aria-hidden="true" />}
              {cover.text}
            </div>
          )}
        </div>
      )}
      {/* A live region has to exist before its content changes to be announced, so the cover's
          text is announced from this one, which stays mounted. The loading and reconnecting covers
          are hidden from assistive technology; the disconnected one is not, so that its button
          stays reachable, and browsing meets its message a second time. */}
      <div role="status" className="sr-only">
        {cover?.text}
      </div>
      {!session && (
        <div className="absolute inset-0 flex items-center justify-center bg-panel">
          <EmptyPanel icon={SquareTerminal} message={t("terminal.empty")} />
        </div>
      )}
      {/* Floats over the terminal rather than covering it: a session that has just ended keeps its
          last output on screen (`TerminalController.detach`), and that output is what says why. The
          live region stays mounted and only its content comes and goes, as a live region has to
          exist before its content changes to be announced. */}
      <div role="status" className="pointer-events-none absolute inset-x-0 bottom-4 flex justify-center">
        {session && isDormant && !resuming && (
          <Surface className="pointer-events-auto flex items-center gap-3 rounded-lg border border-separator px-3 py-2 text-sm shadow-lg">
            <span className="text-muted">
              {t(session.status === "archived" ? "terminal.archived" : "terminal.notRunning")}
            </span>
            <Button size="sm" variant="primary" preventFocusOnPress onPress={() => onResume(session.id)}>
              {t(session.status === "archived" ? "sidebar.session.reopen" : "terminal.resume")}
            </Button>
          </Surface>
        )}
      </div>
    </div>
  );
}
