import { Spinner, Surface } from "@heroui/react";
import { Bot, BotOff, FolderPlus, MessageSquarePlus, Plus, SquareTerminal, Unplug } from "lucide-react";
import React, { useCallback, useEffect, useImperativeHandle, useLayoutEffect, useRef, useState } from "react";

import "@xterm/xterm/css/xterm.css";
import { Button } from "../components/Button";
import type { NoAgentReason } from "../agents";
import { reportExternalLinkFailure } from "../components/ExternalLink";
import { EmptyPanel } from "../components/EmptyPanel";
import { PATH_VAR_PARAMS } from "../components/literalParams";
import { focusTopBar } from "../components/regionFocus";
import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../components/StatusAnnouncer";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { useLatest } from "../components/useLatest";
import { Message, useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { isDormant as isDormantStatus, isLive, type Session } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { useOctoboardTheme } from "../theme";
import type { ViewedFile } from "../browser/TerminalFileViewer";
import { TerminalController, type TermStatus } from "./TerminalController";
import { openPathTarget } from "./openPathTarget";
import { programCopyHandler } from "./osc52";
import { SessionHistoryView } from "./SessionHistoryView";
import { TerminalEditMenu } from "./TerminalEditMenu";
import { TerminalLinkTooltip } from "./TerminalLinkTooltip";
import { MAX_AUTO_RECONNECT_ATTEMPTS, reconnectDelay, STABLE_CONNECTION_MS } from "./reconnectPolicy";
import type { HoveredTerminalLink } from "./terminalLinks";
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
  /** A modal dialog is open over the window, which hides the pane from assistive technology: the
   * dormant bar's sentence is written once it has closed and its focus move has been spoken. */
  modalOpen?: boolean;
  /** Adds a project when the current console already has a console session but no projects. */
  onAddProject?: () => void;
  /** Offers session creation for the current console and focus scope. */
  newSession?: { message: string; label: string; onPress: () => void };
  /** No agent is both added and installed, so creating a session cannot be offered; `reason` says
   * whether the user has to add one or install one. */
  noAgents?: NoAgentReason;
  /** Opens Settings on the Add agent dialog, for the empty state that asks the user to add an agent. */
  onAddAgent: () => void;
  /** A docked pane beside the terminal is easing, which resizes the terminal every frame. The
   * terminal keeps its size until the ease ends, then fits once: a refit per frame rebuilds
   * xterm's dimension styles each time, which costs a full restyle in WebKit. */
  resizing?: boolean;
  /** Shows a project file a path in the output named, in the file viewer, at its line if the path
   * gave one. Without it such a path opens in the default application. */
  onViewFile?: (file: ViewedFile) => void;
  /** Shows a session as selecting it in the sidebar would: the history of an archived session links
   * to its owner with it. */
  onSelectSession?: (session: Session) => void;
  /** Where focus goes when a control that held it inside the pane goes away (the history's owner
   * link, Reconnect, a creation action), in place of the pane's own terminal, which a panel
   * covering the agent area can leave out of reach. */
  restoreFocus?: () => void;
  ref?: React.Ref<TerminalPaneHandle>;
}

export interface TerminalPaneHandle {
  /** Puts keyboard focus on the terminal, or on the button the pane offers in its place — the
   * cover's Reconnect, or the empty state's creation action — since the terminal's Tab belongs to the
   * agent and this is how the keyboard reaches either. With nothing to focus it goes to the top
   * bar, except from `<body>`, which is left alone (`focusTerminal`). */
  focus: () => void;
}

function fitController(controller: TerminalController): void {
  controller.fit();
  controller.syncSize();
}

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
  modalOpen = false,
  onAddProject,
  newSession,
  noAgents,
  onAddAgent,
  resizing = false,
  onViewFile,
  onSelectSession,
  restoreFocus,
  ref,
}: TerminalPaneProps): React.ReactElement {
  const t = useT();
  const { terminalUrl, terminalReconnectDelay, request, toastError } = useDaemon();
  const { links, clipboard } = usePlatform();
  const { resolved: colorTheme } = useOctoboardTheme();
  const containerRef = useRef<HTMLDivElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const coverRef = useRef<HTMLDivElement>(null);
  const emptyRef = useRef<HTMLDivElement>(null);
  const controllerRef = useRef<TerminalController | undefined>(undefined);
  const resizingRef = useLatest(resizing);
  const [status, setStatus] = useState<TermStatus>("closed");
  const [painted, setPainted] = useState(false);
  // Where the OSC 8 link hover goes: the tooltip holds the state itself, so hovering does not render the pane.
  const setLinkRef = useRef<((link: HoveredTerminalLink | undefined) => void) | undefined>(undefined);
  // How the selected session's connection is doing once an attempt has failed (`undefined` while
  // it is healthy or has not failed yet); `reconnecting` until the automatic attempts are spent.
  // Stored with the session it was worked out for, so that the first render after a session switch
  // does not show the previous session's.
  const [troubleOf, setTroubleOf] = useState<{ session: string; state: "reconnecting" | "disconnected" }>();
  const troubleIsCurrent = troubleOf?.session === session?.id;
  const trouble = troubleIsCurrent ? troubleOf?.state : undefined;
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
  const hasSessionRef = useLatest(session !== undefined);
  // An archived session's terminal is covered by its history and inert, so focus asked for the
  // terminal goes to the history's scrolling region instead, or while there is none (loading, no
  // messages, a failed read) to its first control below the header (the header's owner link is not
  // where the terminal's focus belongs), the history itself, then the bar's Reopen.
  const isArchived = session?.status === "archived";
  const archivedRef = useLatest(isArchived);
  const historyRef = useRef<HTMLDivElement>(null);
  const barRef = useRef<HTMLDivElement>(null);
  const focusTerminal = useCallback(() => {
    if (archivedRef.current) {
      const history = historyRef.current;
      (
        history?.querySelector<HTMLElement>("[role=region][tabindex]") ??
        history?.querySelector<HTMLElement>(":is(input, button):not(header *)") ??
        history?.querySelector<HTMLElement>("section[tabindex]") ??
        barRef.current?.querySelector<HTMLElement>("button")
      )?.focus();
    } else if (hasSessionRef.current) controllerRef.current?.focus();
    else if (document.activeElement !== document.body) focusTopBar();
  }, [hasSessionRef, archivedRef]);
  // The session going away turns the terminal inert, and focus inside it would then fall to
  // `<body>` with no event; hand it to the top bar first.
  const noSession = session === undefined;
  useLayoutEffect(() => {
    if (noSession && rootRef.current?.contains(document.activeElement)) focusTopBar();
  }, [noSession]);
  // The session on screen turning archived makes the terminal inert under focus, which falls to
  // `<body>` with no event; it goes to the history first.
  useLayoutEffect(() => {
    if (isArchived && rootRef.current?.contains(document.activeElement)) focusTerminal();
  }, [isArchived, focusTerminal]);
  // An account switch holds the session: the terminal is covered and inert for its whole length, so
  // no key reaches the process, and focus that was in it goes back once it is released.
  const switching = session?.switching_account === true && !isArchived;
  const focusWasInTerminalRef = useRef(false);
  useLayoutEffect(() => {
    if (switching) focusWasInTerminalRef.current = rootRef.current?.contains(document.activeElement) ?? false;
    else if (focusWasInTerminalRef.current) {
      focusWasInTerminalRef.current = false;
      if (document.activeElement === document.body) focusTerminal();
    }
  }, [switching, focusTerminal]);
  const held = useFocusHandoff(restoreFocus ?? focusTerminal);

  useImperativeHandle(
    ref,
    () => ({
      focus: () => {
        const button = coverRef.current?.querySelector("button");
        if (button) {
          button.focus();
          return;
        }
        // The empty state's action button is this region's only control while it is offered,
        // so focus asked for the terminal goes there rather than out to the top bar — it is what
        // F6 lands on, which `App` makes the region a stop for. Not from `<body>`, as
        // `focusTerminal` has it: `refocusIfLost` asks at mount, and launching must not pull focus
        // onto a control.
        const action = emptyRef.current?.querySelector("button");
        if (action && document.activeElement !== document.body) action.focus();
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
  const translateRef = useLatest(t);
  // The paths in the output are asked about for the session on screen, which the controller built
  // once does not otherwise follow.
  const requestRef = useLatest(request);
  const toastErrorRef = useLatest(toastError);
  const projectIdRef = useLatest(session?.project_id);
  const onViewFileRef = useLatest(onViewFile);

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
        openLink: links && ((url) => links.open(url).catch((cause: unknown) => reportExternalLinkFailure(translateRef.current, url, cause))),
        onLinkHover: (link) => setLinkRef.current?.(link),
        copyToClipboard: programCopyHandler(clipboard, () => ({ t: translateRef.current, toastError: toastErrorRef.current })),
        pathLinks: {
          probe: async (paths) => {
            const reply = await requestRef.current({ type: "probe_path_links", project: projectIdRef.current ?? undefined, paths });
            return reply.type === "path_links" ? reply.links.map(({ kind, view, viewer_only }) => ({ kind, view, viewerOnly: viewer_only })) : [];
          },
          open: (target, onHost) =>
            openPathTarget({
              target,
              onHost,
              project: projectIdRef.current ?? undefined,
              request: requestRef.current,
              viewFile: onViewFileRef.current,
              toastError: toastErrorRef.current,
            }),
        },
      },
      appliedColorThemeRef.current,
    );
    controllerRef.current = controller;
    if (containerRef.current) controller.mount(containerRef.current);

    const resizeObserver = new ResizeObserver(() => {
      // The observer does not fire again after the last size of the ease, so the effect below
      // fits once the ease has ended.
      if (resizingRef.current) return;
      fitController(controller);
    });
    if (containerRef.current) resizeObserver.observe(containerRef.current);

    return () => {
      if (stableTimerRef.current) clearTimeout(stableTimerRef.current);
      resizeObserver.disconnect();
      controller.dispose();
    };
    // `handleStatusChange` is stable (its own deps are empty); this effect is mount/unmount-only.
  }, []);

  const wasResizingRef = useRef(false);
  useEffect(() => {
    const ended = wasResizingRef.current && !resizing;
    wasResizingRef.current = resizing;
    if (!ended) return;
    if (controllerRef.current) fitController(controllerRef.current);
  }, [resizing]);

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
  // A starting session's agent is still being launched, so there is no process to connect to and
  // nothing saved to show. The pane waits, covered, for the status that says it is up, and
  // attaches then.
  const isStarting = session?.status === "starting";
  // Whether the session's socket should currently be up at all, per the session's own stored
  // status rather than the controller's last-known socket outcome — a reattach can land on
  // `not_running` by losing a race against the session genuinely ending, so only the authoritative
  // `Session` record decides between offering Resume (truly dormant) and Reconnect (still live,
  // just not connected right now).
  const sessionIsLive = session ? isLive(session.status) && !isStarting : false;

  // Keyed on the id alone, so a status change does not produce a new callback.
  const attachedId = session?.id;
  const attachCurrent = useCallback(
    (userInitiated: boolean) => {
      const controller = controllerRef.current;
      if (!controller || attachedId === undefined) return;
      controller.attach(attachedId, terminalUrl(attachedId), userInitiated);
    },
    [attachedId, terminalUrl],
  );

  useEffect(() => {
    reconnectAttemptsRef.current = 0;
  }, [session?.id]);

  useEffect(() => {
    const controller = controllerRef.current;
    if (!controller) return;
    // A dormant session is resumed by the caller before it is ever attached to directly (selecting
    // an interrupted one, or reopening an archived one) — the daemon has no process to
    // connect the terminal socket to for a session with no running process
    // (`apps/daemon/PROTOCOL.md`), only the output its last one left. Once the resume's
    // `session_upserted` broadcast flips the status, this effect re-runs and attaches for real.
    if (session && isStarting) {
      // Nothing to connect to yet, and the broadcast that ends it re-runs this effect, which
      // attaches for real. The screen is cleared: it may hold the session shown before.
      reconnectAttemptsRef.current = 0;
      controller.detach({});
      return;
    }
    if (session && isArchived) {
      // The history view stands in for the screen, so no saved output is loaded behind it, and
      // nothing typed reaches the terminal to reopen the session: the Reopen button does.
      reconnectAttemptsRef.current = 0;
      controller.detach({});
      return;
    }
    if (!session || isDormant) {
      // `detach` loads the saved screen once on entering dormancy, then keeps that snapshot.
      // Whatever attempts were spent belonged to the process that is gone.
      reconnectAttemptsRef.current = 0;
      controller.detach({
        keepScreenFor: session?.id,
        savedOutputUrl: session ? terminalUrl(session.id) : undefined,
      });
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
  }, [session, isDormant, isArchived, isStarting, attachCurrent, terminalUrl]);

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
    const delay = reconnectDelay(reconnectAttemptsRef.current, terminalReconnectDelay);
    const timer = setTimeout(() => {
      reconnectAttemptsRef.current += 1;
      attachCurrent(false);
    }, delay);
    return () => clearTimeout(timer);
    // Deliberately keyed on the session's id rather than its record: a status broadcast arriving
    // for this session would otherwise tear the pending timer down and start it over, so under a
    // working agent the reconnect could be postponed indefinitely.
  }, [status, session?.id, isDormant, sessionIsLive, attachCurrent, terminalReconnectDelay]);

  // A reopen that is refused ends with the session still archived and its history no longer
  // inert; focus that fell to `<body>` while it was goes back to the history.
  const wasResumingRef = useRef(false);
  useLayoutEffect(() => {
    const ended = wasResumingRef.current && !resuming;
    wasResumingRef.current = resuming;
    if (ended && archivedRef.current && document.activeElement === document.body) focusTerminal();
  }, [resuming, archivedRef, focusTerminal]);

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

  // What covers the terminal. A switch of the session's account comes first, as its process going
  // down and coming back up is no trouble with the connection. A connection in trouble comes next,
  // over any old output as well: a terminal that is not connected has nothing live to show, and the
  // disconnected state carries the Reconnect. Otherwise, from selecting a session until its first
  // output is on screen — through a resume starting the process, the socket connecting, and a
  // freshly started agent drawing its first frame — the blank terminal is covered by a loading
  // state. An agent that stays silent does not keep it up forever: once the socket has been open
  // for `LOADING_LIMIT_MS` the bare terminal is shown. The count only runs while the socket is
  // open, so the limit never uncovers a terminal that is not connected (a first connect that
  // hangs, a manual Reconnect under way, a resume still starting the process).
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
  // The bar offering to resume a dormant session or reopen an archived one.
  const dormantBar =
    session && isDormant && !resuming && !switching
      ? session.status === "archived"
        ? { sessionId: session.id, sentence: t("terminal.archived"), action: t("sidebar.session.reopen") }
        : { sessionId: session.id, sentence: t("terminal.notRunning"), action: t("terminal.resume") }
      : undefined;
  // A sentence that arrives or changes under a modal (selecting an interrupted session on an account
  // whose sharing change did not finish asks first) would be written into a hidden region, and
  // closing the modal moves focus over it, so it waits for the close and the focus move. One already
  // spoken stays in the region through a modal: clearing and rewriting it would say it again.
  const barSentence = dormantBar?.sentence;
  const [bar, setBar] = useState<{ held: boolean; spoken: string | undefined }>({ held: false, spoken: undefined });
  useEffect(() => {
    if (!barSentence) {
      setBar((b) => (b.held || b.spoken !== undefined ? { held: false, spoken: undefined } : b));
      return;
    }
    if (modalOpen) {
      if (bar.spoken !== barSentence) setBar({ held: true, spoken: bar.spoken });
      return;
    }
    if (!bar.held) {
      if (bar.spoken !== barSentence) setBar({ held: false, spoken: barSentence });
      return;
    }
    const timer = setTimeout(() => setBar({ held: false, spoken: barSentence }), AFTER_FOCUS_MOVE_DELAY_MS);
    return () => clearTimeout(timer);
  }, [barSentence, modalOpen]);
  const barSaid = modalOpen || bar.held ? (bar.spoken === barSentence ? barSentence : undefined) : barSentence;
  const cover: { text: string; spinner: boolean } | undefined = switching
    ? { text: t("terminal.switchingAccount"), spinner: true }
    : trouble
      ? { text: t(trouble === "disconnected" ? "terminal.disconnected" : "terminal.reconnecting"), spinner: trouble === "reconnecting" }
      : loading
        ? { text: t(resuming ? "terminal.resuming" : isStarting ? "terminal.starting" : "terminal.loading"), spinner: true }
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
      style={{
        "--terminal-background": XTERM_THEMES[colorTheme].background,
        backgroundColor: "var(--terminal-background)",
      } as React.CSSProperties}
    >
      {/* The padding sits on a wrapper, not on the element xterm is mounted in: the fit addon sizes
          the terminal from that element's computed height and width, padding included, so padding
          there gives it rows and columns that the overflow then clips. Isolation keeps xterm's
          transparent link canvas below the pane's overlays so it cannot intercept their clicks. */}
      <div inert={!session || isArchived || switching} className="isolate min-h-0 flex-1 px-3 py-2">
        {/* xterm lays its cells out left to right whatever the page's direction. */}
        <div dir="ltr" className="h-full overflow-hidden" ref={containerRef} />
      </div>
      {session && !isArchived && <TerminalEditMenu key={session.id} sessionId={session.id} controllerRef={controllerRef} contextTargetRef={containerRef} />}
      <TerminalLinkTooltip setLinkRef={setLinkRef} />
      {session && isArchived && (
        // Under the cover, which a resume raises over it; inert meanwhile, so what is hidden is
        // out of reach too.
        <div ref={historyRef} {...held} inert={resuming} className="contents">
          <SessionHistoryView key={session.id} session={session} onSelectSession={onSelectSession} />
        </div>
      )}
      {cover && (
        <div
          ref={coverRef}
          {...held}
          className="absolute inset-0 flex flex-col items-center justify-center-safe text-sm text-app-muted"
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
      {/* The cover's text is spoken from a status region that is mounted empty and written after
          its first delay (`StatusAnnouncer`). The loading and reconnecting covers are hidden from
          assistive technology; the disconnected one is not, so that its button stays reachable, and
          browsing meets its message a second time. */}
      <StatusAnnouncer text={cover?.text} />
      {!session && (
        // `held`: a creation button goes away when its list gains content, from this window
        // or another, and a focused element that is removed leaves focus on `<body>`. It is the
        // same hand-off the cover's button uses, which can never be on screen at the same time.
        <div ref={emptyRef} {...held} className="absolute inset-0 flex items-center justify-center-safe bg-panel">
          {noAgents ? (
            noAgents === "add" ? (
              <EmptyPanel icon={Bot} message={t("agents.noneAdded")} description={t("agents.addPrompt")} action={{ label: t("agents.addAction"), onPress: onAddAgent }} />
            ) : (
              <EmptyPanel icon={BotOff} message={<Message id="agents.noneFound" params={PATH_VAR_PARAMS} />} description={<Message id="welcome.toolsHint" params={PATH_VAR_PARAMS} />} />
            )
          ) : newSession ? (
            <EmptyPanel
              icon={MessageSquarePlus}
              message={newSession.message}
              description={t("terminal.noSessions.description")}
              action={{ ...newSession, icon: Plus }}
            />
          ) : onAddProject ? (
            <EmptyPanel
              icon={FolderPlus}
              message={t("sidebar.noProjects")}
              description={t("terminal.noProjects.withAgent.description")}
              action={{ label: t("sidebar.console.addProject"), icon: FolderPlus, onPress: onAddProject }}
            />
          ) : (
            <EmptyPanel icon={SquareTerminal} message={t("terminal.empty")} description={t("terminal.empty.description")} />
          )}
        </div>
      )}
      {/* Floats over the terminal rather than covering it: a session that has just ended keeps its
          last output on screen (`TerminalController.detach`), and that output is what says why. Its
          sentence is spoken from a status region that stays mounted while the bar comes and goes (the
          pane never opens on a dormant session); the visible sentence is hidden from assistive
          technology so nothing is said twice, and the button stays reachable. */}
      <StatusAnnouncer text={barSaid} />
      <div className="pointer-events-none absolute inset-x-0 bottom-4 flex justify-center">
        {dormantBar && (
          <Surface ref={barRef} className="pointer-events-auto flex items-center gap-3 rounded-lg px-3 py-2 text-sm shadow-lg">
            <span aria-hidden="true" className="text-app-muted">
              {dormantBar.sentence}
            </span>
            <Button size="sm" variant="primary" preventFocusOnPress onPress={() => onResume(dormantBar.sessionId)}>
              {dormantBar.action}
            </Button>
          </Surface>
        )}
      </div>
    </div>
  );
}
