import { Bot, BotOff, FolderPlus, MessageSquarePlus, Plus, SquareTerminal } from "lucide-react";
import React, { useCallback, useEffect, useImperativeHandle, useLayoutEffect, useRef, useState } from "react";

import "@xterm/xterm/css/xterm.css";
import type { NoAgentReason } from "../agents/agents";
import { EmptyPanel } from "../components/panel/EmptyPanel";
import { PATH_VAR_PARAMS } from "../components/text/literalParams";
import { focusTopBar } from "../components/focus/regionFocus";
import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../components/announce/StatusAnnouncer";
import { useFocusHandoff } from "../components/focus/useFocusHandoff";
import { useLatest } from "../components/hooks/useLatest";
import { Message, useT } from "../i18n/react";
import { type Session } from "../protocol";
import { sessionDisplayTitle } from "../sessions/sessionLabel";
import { hasNoProcess, holdsPendingLink, isLive } from "../sessions/sessionStatus";
import { useAccountSwitchHeld, useDaemon } from "../store";
import type { ViewedFile } from "../viewer/viewedFile";
import type { TerminalController } from "./TerminalController";
import { SessionHistoryView } from "./history/SessionHistoryView";
import { TerminalEditMenu } from "./TerminalEditMenu";
import { TerminalSurface } from "./TerminalSurface";
import { TerminalCover } from "./TerminalCover";
import { TerminalNoticeBar, TerminalNoticeSlot } from "./TerminalNoticeBar";
import { useAnnouncedOnRise } from "./useAnnouncedOnRise";
import { useAnnouncedWhilePending } from "./useAnnouncedWhilePending";
import { usePendingFocus } from "./usePendingFocus";
import { useTerminalController } from "./useTerminalController";
import { useTerminalReconnect } from "./useTerminalReconnect";
import type { HoveredTerminalLink } from "./links/terminalLinks";

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
   * no-process bar's sentence is written once it has closed and its focus move has been spoken. */
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
  /** Puts keyboard focus on the terminal, or on what the pane offers in its place — the cover's
   * Reconnect, the pending cover's region, or the empty state's creation action — since the terminal's
   * Tab belongs to the agent and this is how the keyboard reaches any of them. With nothing to focus
   * it goes to the top bar, except from `<body>`, which is left alone (`focusTerminal`). */
  focus: () => void;
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
  const { terminalUrl, terminalReconnectDelay } = useDaemon();
  const containerRef = useRef<HTMLDivElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const coverRef = useRef<HTMLDivElement>(null);
  const emptyRef = useRef<HTMLDivElement>(null);
  const controllerRef = useRef<TerminalController | undefined>(undefined);
  const [painted, setPainted] = useState(false);
  // Where the OSC 8 link hover goes: the tooltip holds the state itself, so hovering does not render the pane.
  const setLinkRef = useRef<((link: HoveredTerminalLink | undefined) => void) | undefined>(undefined);
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
  // A live session whose process the daemon cannot reach (`holder_link`) has its terminal covered
  // and inert, and focus asked for it goes to the cover's region, which says why.
  const livePending = session !== undefined && holdsPendingLink(session);
  const pendingRef = useLatest(livePending);
  const focusTerminal = useCallback(() => {
    if (pendingRef.current) coverRef.current?.focus();
    else if (archivedRef.current) {
      const history = historyRef.current;
      (
        history?.querySelector<HTMLElement>("[role=region][tabindex]") ??
        history?.querySelector<HTMLElement>(":is(input, button):not(header *)") ??
        history?.querySelector<HTMLElement>("section[tabindex]") ??
        barRef.current?.querySelector<HTMLElement>("button")
      )?.focus();
    } else if (hasSessionRef.current) controllerRef.current?.focus();
    else if (document.activeElement !== document.body) focusTopBar();
  }, [hasSessionRef, archivedRef, pendingRef]);
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
  const switchHeld = useAccountSwitchHeld(session);
  const switching = switchHeld && !isArchived;
  const focusWasInTerminalRef = useRef(false);
  useLayoutEffect(() => {
    if (switching) focusWasInTerminalRef.current = rootRef.current?.contains(document.activeElement) ?? false;
    else if (focusWasInTerminalRef.current) {
      focusWasInTerminalRef.current = false;
      if (document.activeElement === document.body) focusTerminal();
    }
  }, [switching, focusTerminal]);
  const held = useFocusHandoff(restoreFocus ?? focusTerminal);
  // Selecting another archived session replaces the history, and focus in it falls to `<body>` (a
  // sidebar row's press leaves focus where it was). The user made that move, so focus goes where
  // focus meant for the terminal goes, with the modality the selecting input left: no ring after a
  // pointer press, unlike `held`, which turns the ring on for a control that went away by itself.
  // `held` cannot tell the two apart, so this runs first, as a layout effect, and leaves it nothing
  // to do. Only towards another archived session: a live one's terminal is where the keyboard works
  // next, as for any other handoff. The flag goes while no history is shown or it is inert, as
  // focus can leave it then without a blur.
  const historyFocusedRef = useRef(false);
  useLayoutEffect(() => {
    if (!historyFocusedRef.current || document.activeElement !== document.body) return;
    historyFocusedRef.current = false;
    if (archivedRef.current) (restoreFocus ?? focusTerminal)();
  }, [session?.id]);
  useLayoutEffect(() => {
    if (!isArchived || resuming) historyFocusedRef.current = false;
  }, [isArchived, resuming]);

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
  const noProcess = session ? hasNoProcess(session.status) : false;
  // A starting session's agent is still being launched, so there is no process to connect to and
  // nothing saved to show. The pane waits, covered, for the status that says it is up, and
  // attaches then.
  const isStarting = session?.status === "starting";
  // Whether the session's socket should currently be up at all, per the session's own stored
  // status rather than the controller's last-known socket outcome — a reattach can land on
  // `not_running` by losing a race against the session genuinely ending, so only the authoritative
  // `Session` record decides between offering Resume (truly without a process) and Reconnect (still live,
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

  // Before `useTerminalReconnect`, which reads what it found as the suspension ends.
  const { coverHasFocus, focusReturned } = usePendingFocus({
    pending: livePending,
    rootRef,
    coverRef,
    focusTerminal,
    focusAfterTap: restoreFocus ?? focusTopBar,
  });
  const { status, onStatusChange, resetAttempts, attemptsSpent, trouble, canReconnect, reconnectNow } = useTerminalReconnect({
    controllerRef,
    initialStatus: "closed",
    targetId: session?.id,
    live: !noProcess && sessionIsLive,
    suspended: livePending,
    recoveredWithUser: focusReturned,
    attach: attachCurrent,
    terminalReconnectDelay,
  });
  useTerminalController({
    controllerRef,
    containerRef,
    setLinkRef,
    resizing,
    onStatusChange,
    onPaintedChange: setPainted,
    inputHeld: livePending,
    project: session?.project_id,
    viewFile: onViewFile,
  });
  useEffect(() => {
    const controller = controllerRef.current;
    if (!controller) return;
    // A session without a process is resumed by the caller before it is ever attached to directly (selecting
    // an interrupted one, or reopening an archived one) — the daemon has no process to
    // connect the terminal socket to for a session with no running process
    // (`apps/daemon/PROTOCOL.md`), only the output its last one left. Once the resume's
    // `session_upserted` broadcast flips the status, this effect re-runs and attaches for real.
    if (session && isStarting) {
      // Nothing to connect to yet, and the broadcast that ends it re-runs this effect, which
      // attaches for real. The screen is cleared: it may hold the session shown before.
      resetAttempts();
      controller.detach({});
      return;
    }
    if (session && isArchived) {
      // The history view stands in for the screen, so no saved output is loaded behind it, and
      // nothing typed reaches the terminal to reopen the session: the Reopen button does.
      resetAttempts();
      controller.detach({});
      return;
    }
    if (!session || noProcess) {
      // `detach` loads the saved screen once when the process is gone, then keeps that snapshot.
      // Whatever attempts were spent belonged to the process that is gone.
      resetAttempts();
      controller.detach({
        keepScreenFor: session?.id,
        savedOutputUrl: session ? terminalUrl(session.id) : undefined,
      });
      return;
    }
    if (livePending) {
      // The daemon cannot reach the process, so nothing is attached and no attempt is spent: the
      // hook attaches once when it can. A screen of another session is cleared; this session's own
      // output stays under the cover.
      if (controller.currentSessionId !== session.id) controller.detach({ keepScreenFor: session.id });
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
    if (controller.currentSessionId === session.id && attemptsSpent()) return;
    attachCurrent(true);
  }, [session, noProcess, isArchived, isStarting, attachCurrent, terminalUrl]);

  // A reopen that is refused ends with the session still archived and its history no longer
  // inert; focus that fell to `<body>` while it was goes back to the history.
  const wasResumingRef = useRef(false);
  useLayoutEffect(() => {
    const ended = wasResumingRef.current && !resuming;
    wasResumingRef.current = resuming;
    if (ended && archivedRef.current && document.activeElement === document.body) focusTerminal();
  }, [resuming, archivedRef, focusTerminal]);

  // What covers the terminal, in this order. A process the daemon cannot reach comes first of all, over
  // the socket's own state and the old output: nothing on screen is live, and no button is offered, as
  // the daemon alone retries. A switch of the session's account comes next, as its process going down
  // and coming back up is no trouble with the connection. A connection in trouble comes after it,
  // over any old output as well: a terminal that is not connected has nothing live to show, and the
  // disconnected state carries the Reconnect. Otherwise, from selecting a session until its first
  // output is on screen — through a resume starting the process, the socket connecting, and a
  // freshly started agent drawing its first frame — the blank terminal is covered by a loading
  // state. An agent that stays silent does not keep it up forever: once the socket has been open
  // for `LOADING_LIMIT_MS` the bare terminal is shown. The count only runs while the socket is
  // open, so the limit never uncovers a terminal that is not connected (a first connect that
  // hangs, a manual Reconnect under way, a resume still starting the process).
  const awaitingOutput = session !== undefined && !painted && (resuming || !noProcess);
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
  // The bar offering to resume a session without a process or reopen an archived one.
  const noProcessBar =
    session && noProcess && !resuming && !switching
      ? session.status === "archived"
        ? { sessionId: session.id, sentence: t("terminal.archived"), action: t("sidebar.session.reopen") }
        : { sessionId: session.id, sentence: t("terminal.notRunning"), action: t("terminal.resume") }
      : undefined;
  // A sentence that arrives or changes under a modal (selecting an interrupted session on an account
  // whose sharing change did not finish asks first) would be written into a hidden region, and
  // closing the modal moves focus over it, so it waits for the close and the focus move. One already
  // spoken stays in the region through a modal: clearing and rewriting it would say it again.
  const barSentence = noProcessBar?.sentence;
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
  const pendingCover = livePending && session ? { label: sessionDisplayTitle(t, session), title: t("terminal.holderPending.title"), body: t("terminal.holderPending.body") } : undefined;
  const pendingSaid = useAnnouncedWhilePending(session?.id, livePending, t("terminal.holderPending.sentence"), coverHasFocus);
  const cover: { text: string; spinner: boolean } | undefined = pendingCover
    ? { text: pendingCover.title, spinner: true }
    : switching
      ? { text: t("terminal.switchingAccount"), spinner: true }
      : trouble
        ? { text: t(trouble === "disconnected" ? "terminal.disconnected" : "terminal.reconnecting"), spinner: trouble === "reconnecting" }
        : loading
          ? { text: t(resuming ? "terminal.resuming" : isStarting ? "terminal.starting" : "terminal.loading"), spinner: true }
          : undefined;

  // The mark of a rebuilt display is drawn in the title bar beside the session's title; the pane
  // speaks it when the flag turns on under an open session, not for one that is shown with it.
  // A rise under a pending cover waits for it to go, and for the cover that follows it (the loading
  // one, until the first paint) to go too, so that it overwrites neither of their sentences.
  const [afterPending, setAfterPending] = useState(false);
  const covered = cover !== undefined;
  useLayoutEffect(() => {
    if (livePending) setAfterPending(true);
    else if (!covered) setAfterPending(false);
  }, [livePending, covered]);
  const recoveredSaid = useAnnouncedOnRise(session?.id, session?.screen_inexact === true, t("terminal.recovered.title"), livePending || afterPending);

  return (
    // `data-escape-scope`: one of the origins `usePaneToggles`'s capture-phase Escape listener
    // closes a drawer for — xterm.js holds keyboard focus here most of the time and would otherwise
    // swallow the key before that listener's own bubble-phase alternative ever saw it.
    <TerminalSurface
      ref={rootRef}
      data-escape-scope
      data-region="terminal"
      className="relative flex min-h-0 min-w-0 flex-1 flex-col"
      containerRef={containerRef}
      setLinkRef={setLinkRef}
      padding="session-terminal-inset py-2"
      inert={!session || isArchived || switching || livePending}
      editMenu={session && !isArchived && <TerminalEditMenu key={session.id} sessionId={session.id} controllerRef={controllerRef} contextTargetRef={containerRef} />}
    >
      {session && isArchived && (
        // Under the cover, which a resume raises over it; inert meanwhile, so what is hidden is
        // out of reach too.
        <div
          ref={historyRef}
          onFocus={(event) => { historyFocusedRef.current = true; held.onFocus(event); }}
          onBlur={(event) => {
            if (document.activeElement !== event.target) historyFocusedRef.current = false;
            held.onBlur(event);
          }}
          inert={resuming}
          className="contents"
        >
          <SessionHistoryView key={session.id} session={session} onSelectSession={onSelectSession} restoreFocus={restoreFocus ?? focusTerminal} />
        </div>
      )}
      {cover && (
        <TerminalCover
          ref={coverRef}
          rootRef={rootRef}
          held={held}
          text={cover.text}
          spinner={cover.spinner}
          disconnected={trouble === "disconnected"}
          canReconnect={canReconnect}
          onReconnect={reconnectNow}
          pending={pendingCover}
        />
      )}
      {/* The cover's text is spoken from a status region that is mounted empty and written after
          its first delay (`StatusAnnouncer`). The loading and reconnecting covers are hidden from
          assistive technology; the disconnected one and the pending one are not, so that the button
          and the focusable region stay reachable, and browsing meets their message a second time. */}
      <StatusAnnouncer text={livePending ? pendingSaid : cover?.text} />
      {!session && (
        // `held`: a creation button goes away when its list gains content, from this window
        // or another, and a focused element that is removed leaves focus on `<body>`. It is the
        // same handoff the cover's button uses, which can never be on screen at the same time.
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
          pane never opens on a session without a process); the visible sentence is hidden from assistive
          technology so nothing is said twice, and the button stays reachable. */}
      <StatusAnnouncer text={barSaid} />
      <StatusAnnouncer text={recoveredSaid} clearAfterRead />
      <TerminalNoticeSlot>
        {noProcessBar && (
          <TerminalNoticeBar
            ref={barRef}
            sentence={noProcessBar.sentence}
            action={noProcessBar.action}
            variant="primary"
            onPress={() => onResume(noProcessBar.sessionId)}
          />
        )}
      </TerminalNoticeSlot>
    </TerminalSurface>
  );
}
