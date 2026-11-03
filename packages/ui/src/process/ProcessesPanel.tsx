import { Funnel, FunnelX, SquareActivity } from "lucide-react";
import React, { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";

import { afterCollectionFocus } from "../components/focus/afterCollectionFocus";
import { Button } from "../components/controls/Button";
import { CenteredEmptyPanel } from "../components/panel/EmptyPanel";
import { ErrorAlert } from "../components/alerts/ErrorAlert";
import { errorDetails } from "../components/alerts/errorParts";
import { HOVER_ACTIONS_CLASS } from "../components/rows/hoverActions";
import { FadeOverflow } from "../components/text/FadeOverflow";
import { focusFellToPage, focusLanding } from "../components/focus/regionFocus";
import { PaneFailure } from "../components/alerts/PaneFailure";
import { IconToggle } from "../components/controls/IconToggle";
import { PanelHeader } from "../components/panel/PanelHeader";
import { ProcessCommand, ProcessTreeDetails, StopGlyph } from "./ProcessTreeDetails";
import { FadedScroller, useFadeRegion } from "../components/scroll/scrollFade";
import { StickyHeading } from "../components/scroll/StickyHeading";
import { useStickyHeadingFocus } from "../components/scroll/stickyHeadingFocus";
import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../components/announce/StatusAnnouncer";
import { useFocusHandoff } from "../components/focus/useFocusHandoff";
import { useListMotion } from "../components/motion/useListMotion";
import type { PlainMessageKey } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import type { ErrorPresentation } from "../errors/errorPresentation";
import { keepProcesses, stopProcesses } from "./processes";
import type { ProcessTree } from "../protocol";
import { sessionDisplayTitle } from "../sessions/sessionLabel";
import { useSession } from "../store";
import { useDaemon } from "../store";
import { ringForAutomaticFocus } from "../input/touchInput";
import { ConfirmDialog } from "../components/dialog/ConfirmDialog";
import { useProcessPanelContext } from "./useProcessPanel";
import { ProcessSessionSelect } from "./ProcessSessionSelect";

export function ProcessesPanel(): React.ReactElement | null {
  const data = useProcessPanelContext();
  return data ? <ProcessesContent key={data.ownerKey} data={data} /> : null;
}

/** The two things a row can ask of a tree, each behind a confirmation: what it says, and what it
 * sends. */
type ProcessAction = "stop" | "keep";

const ACTIONS = {
  stop: { title: "processes.stopTitle", message: "processes.stopMessage", confirm: "processes.stop", failedTitle: "processes.stopFailedTitle", done: "processes.stopped", send: stopProcesses },
  keep: { title: "processes.keepTitle", message: "processes.keepMessage", confirm: "processes.keep", failedTitle: "processes.keepFailedTitle", done: "processes.kept", send: keepProcesses },
} as const satisfies Record<ProcessAction, { title: PlainMessageKey; message: PlainMessageKey; confirm: PlainMessageKey; failedTitle: PlainMessageKey; done: PlainMessageKey; send: unknown }>;

function ProcessesContent({ data }: { data: NonNullable<ReturnType<typeof useProcessPanelContext>> }): React.ReactElement {
  const t = useT();
  const { showNotice } = useDaemon();
  const { trees, error, retry, settle } = data;
  const scrollRegion = useFadeRegion({ label: t("panel.processes.name") });
  const bodyRef = useRef<HTMLDivElement>(null);
  const scrollerRef = useRef<HTMLDivElement>(null);
  const [stickyHeadingHeight, setStickyHeadingHeight] = useState(0);
  const grouped = data.projectId !== undefined && data.sessionId === undefined;
  // Which category headings the list draws: one for the session trees, one for the project's own.
  const hasSessionTrees = grouped && (trees?.some((tree) => tree.session_id !== undefined) ?? false);
  const hasProjectTrees = grouped && (trees?.some((tree) => tree.session_id === undefined) ?? false);
  const hasSessionNote = !grouped && (trees?.some((tree) => tree.session_id !== undefined) ?? false);
  const hasStickyHeading = hasSessionTrees || hasProjectTrees || hasSessionNote;
  // Notes wrap with the pane width and language; keep focused rows below the whole pinned header.
  // Measured on every commit; the observer follows size changes between commits.
  useLayoutEffect(() => {
    const headings = bodyRef.current?.querySelectorAll<HTMLElement>("[data-sticky-heading]");
    const scroller = scrollerRef.current;
    if (!headings || !scroller) return;
    const measure = () => {
      const top = scroller.getBoundingClientRect().top;
      const heading = Array.from(headings).find((element) => element.getBoundingClientRect().bottom > top);
      setStickyHeadingHeight(heading?.getBoundingClientRect().height ?? 0);
    };
    const observer = new ResizeObserver(measure);
    headings.forEach((heading) => observer.observe(heading));
    scroller.addEventListener("scroll", measure, { passive: true });
    measure();
    return () => {
      observer.disconnect();
      scroller.removeEventListener("scroll", measure);
    };
  });
  const [filterOpen, setFilterOpen] = useState(false);
  const filterToggle = useRef<HTMLButtonElement>(null);
  const filterShown = filterOpen || data.sessionId !== undefined;
  const toggleFilter = (open: boolean) => {
    setFilterOpen(open);
    if (!open) data.selectSession(undefined);
  };
  const failed = trees === undefined && error !== undefined;
  const [selected, setSelected] = useState<{ tree: ProcessTree; action: ProcessAction; nextId?: string }>();
  // A confirmed request that is on its way keeps its confirmation, and with it the success notice and the
  // focus handoff, even when a refresh has already taken the tree out of the list.
  const [pending, setPending] = useState(false);
  const listed = trees?.find((tree) => tree.id === selected?.tree.id);
  const vanished = selected !== undefined && !pending && trees !== undefined && listed === undefined;

  const rowOf = (id: string) => [...(bodyRef.current?.querySelectorAll<HTMLElement>("li[data-list-motion]") ?? [])].find((row) => row.dataset.listMotion === id);
  // Whether the pressed row's Stop or Keep held focus when its confirmation opened: a keyboard opening leaves it
  // there, and so can a pointer press while nothing else held focus (the buttons keep focus where it was, which a
  // browser cannot do for the page itself). The dialog's own return of focus then has nothing to return to once
  // the row is gone, whatever confirmed.
  const opener = useRef(false);
  /** Focus for the panel once a confirmed action opened from the keyboard has taken `tree` out of the list,
   * when the dialog's own return of focus found nothing to return to and focus fell to the page: the first
   * action of the row that followed it, else where F6 lands in the panel, with the focus ring. */
  const focusAfterAction = (nextId: string | undefined) =>
    afterCollectionFocus(() => {
      if (!opener.current || !focusFellToPage()) return;
      ringForAutomaticFocus();
      const next = nextId === undefined ? undefined : rowOf(nextId)?.querySelector<HTMLElement>("button:not(:disabled)");
      const region = bodyRef.current?.closest("[data-region]");
      if (next) next.focus();
      else if (region) focusLanding(region);
    });
  // The tree went out of a refresh while its confirmation was open: the confirmation closes, and its dialog's
  // return of focus finds the opening Stop or Keep gone.
  useEffect(() => {
    if (!vanished) return;
    setSelected(undefined);
    focusAfterAction(selected?.nextId);
  }, [vanished]);
  const onDone = (tree: ProcessTree, action: ProcessAction, ids: string[]) => {
    const nextId = nextIdOf(tree.id) ?? selected?.nextId;
    settle(ids);
    if (ids.length > 0) showNotice(t(ACTIONS[action].done));
    setSelected(undefined);
    focusAfterAction(nextId);
  };
  const nextIdOf = (id: string) => {
    const following = rowOf(id)?.nextElementSibling;
    return following instanceof HTMLElement ? following.dataset.listMotion : undefined;
  };
  // The failure gives way to the loading line when Try again starts, taking its button along: where the button held
  // focus, focus goes to where F6 lands in the panel as the failure goes.
  const retryList = (from: Element | null) => {
    const held = from?.contains(document.activeElement) ?? false;
    flushSync(retry);
    const region = bodyRef.current?.closest("[data-region]");
    if (held && region) {
      ringForAutomaticFocus();
      focusLanding(region);
    }
  };
  const ask = (action: ProcessAction, tree: ProcessTree) => {
    opener.current = rowOf(tree.id)?.contains(document.activeElement) ?? false;
    setSelected({ tree, action, nextId: nextIdOf(tree.id) });
  };
  return (
    <>
      <PanelHeader title={t("panel.processes.name")} actions={data.projectId && (
        <IconToggle ref={filterToggle} landing={!failed && !filterShown} label={t(filterShown ? "processes.sessionFilter.hide" : "processes.sessionFilter")} isSelected={filterShown} expanded={filterShown} onChange={toggleFilter}>
          {filterShown ? <FunnelX aria-hidden="true" className="size-4" /> : <Funnel aria-hidden="true" className="size-4" />}
        </IconToggle>
      )} />
      {data.projectId && <ProcessSessionSelect projectId={data.projectId} value={data.sessionId} onChange={(id) => { setFilterOpen(true); data.selectSession(id); }} landing={!failed} shown={filterShown} opened={filterOpen} onEmptyBlur={() => setFilterOpen(false)} toggleRef={filterToggle} />}
      <div ref={bodyRef} className={`flex min-h-0 flex-1 flex-col ${scrollRegion.insetRing}`}>
        <FadedScroller ref={scrollerRef} region={scrollRegion} stickyHeadingHeight={stickyHeadingHeight} className={`flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-(--pane-inset) ${hasStickyHeading ? "pb-4" : "py-4"}`}>
          {failed ? (
            <PaneFailure title={t("processes.listFailedTitle")} summary={t("processes.listFailed")} details={errorDetails(error)} onRetry={retryList} announce announceDelayMs={AFTER_FOCUS_MOVE_DELAY_MS} landing />
          ) : (
            <ProcessList grouped={grouped} keepable={data.projectId !== undefined} trees={trees} error={error} acting={selected !== undefined} onStop={(tree) => ask("stop", tree)} onKeep={(tree) => ask("keep", tree)} />
          )}
        </FadedScroller>
      </div>
      {selected && (
        <ProcessConfirmation key={`${selected.action}:${selected.tree.id}`} action={selected.action} tree={listed ?? selected.tree}
          onCancel={() => setSelected(undefined)} onPending={setPending} onDone={(ids) => onDone(selected.tree, selected.action, ids)} />
      )}
    </>
  );
}

/** The confirmation of an action on a tree: what is asked, the tree's command, where it works and what
 * it is doing. A refusal is the shared error alert's, with the confirmation staying; a request still
 * on its way when the panel passes to another owner reports its outcome to no one. */
function ProcessConfirmation({ action, tree, onCancel, onPending, onDone }: {
  action: ProcessAction;
  tree: ProcessTree;
  onCancel: () => void;
  onPending: (pending: boolean) => void;
  onDone: (ids: string[]) => void;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    return () => { live.current = false; };
  }, []);
  const words = ACTIONS[action];
  return (
    <ConfirmDialog title={t(words.title)} destructive={action === "stop"} confirmLabel={t(words.confirm)} errorTitle={t(words.failedTitle)} resetKey={tree.id}
      message={<><p>{t(words.message)}</p><div className="flex min-w-0 flex-col gap-2"><ProcessCommand tree={tree} wrap leading={action === "stop" ? <StopGlyph /> : undefined} /><ProcessTreeDetails tree={tree} /></div></>}
      onCancel={onCancel} onConfirm={() => {
        onPending(true);
        return words.send(request, [tree.id]).then(onDone, (error: unknown) => {
          if (live.current) throw error;
        }).finally(() => onPending(false));
      }} />
  );
}

/** An explanation under a heading or leading the list: one size and colour, whatever it explains. */
const NOTE_CLASS = "text-xs text-app-muted";

type Group = { key: string; sessionId?: string; trees: ProcessTree[] };

/** A project's trees by the session that started them, in the order they first appear, then the
 * project processes, which no session owns. */
function groupsOf(trees: ProcessTree[]): Group[] {
  const groups = new Map<string, Group>();
  for (const tree of trees) {
    const key = tree.session_id ?? "";
    const group = groups.get(key) ?? { key: key === "" ? "project-processes" : key, sessionId: tree.session_id, trees: [] };
    group.trees.push(tree);
    groups.set(key, group);
  }
  const owned = groups.get("");
  groups.delete("");
  return [...groups.values(), ...(owned ? [owned] : [])];
}

/** The body: a loading line, the empty state, or the trees, under an Alert when a later refresh
 * failed. While a confirmation is open (`acting`) the buttons of every row are unavailable. A tree that
 * goes away while it or its button holds focus hands focus to the panel. */
function ProcessList({ grouped, keepable, trees, error, acting, onStop, onKeep }: {
  grouped: boolean;
  /** Whether the trees of one session can be kept: not a console session's. */
  keepable: boolean;
  trees: ProcessTree[] | undefined;
  error: ErrorPresentation | undefined;
  acting: boolean;
  onStop: (tree: ProcessTree) => void;
  onKeep: (tree: ProcessTree) => void;
}): React.ReactElement {
  const t = useT();
  const rootRef = React.useRef<HTMLDivElement>(null);
  const handoff = useFocusHandoff(() => {
    const region = rootRef.current?.closest<HTMLElement>("[data-region]");
    if (region) focusLanding(region);
  });
  const loading = trees === undefined && error === undefined;
  const hasNote = !grouped && trees?.some((tree) => tree.session_id !== undefined);
  return (
    <div ref={rootRef} className="flex min-w-0 flex-1 flex-col" {...handoff}>
      {/* Allow the panel's focus announcement to finish before speaking its loading state. */}
      <StatusAnnouncer text={loading ? t("processes.loading") : undefined} firstWriteDelayMs={AFTER_FOCUS_MOVE_DELAY_MS} />
      {hasNote && <StickyProcessNote>{t("processes.panelMessage")}</StickyProcessNote>}
      <div className="flex min-w-0 flex-1 flex-col gap-4">
        {loading && <p aria-hidden="true" className="text-sm text-app-muted">{t("processes.loading")}</p>}
        {trees !== undefined && error !== undefined && (
          <ErrorAlert status="warning" title={t("processes.refreshFailedTitle")} details={errorDetails(error)} announce>
            {t("processes.refreshFailed")}
          </ErrorAlert>
        )}
        {trees?.length === 0 && (
          <CenteredEmptyPanel icon={SquareActivity} message={t("processes.empty")} description={t(grouped ? "processes.empty.allSessions" : "processes.empty.session")} />
        )}
        {trees !== undefined && trees.length > 0 && (!grouped
          ? <TreeList trees={trees} acting={acting} keepable={keepable} onStop={onStop} onKeep={onKeep} />
          : <GroupedTrees trees={trees} acting={acting} onStop={onStop} onKeep={onKeep} />)}
      </div>
    </div>
  );
}

function StickyProcessNote({ children }: { children: React.ReactNode }): React.ReactElement {
  const { className, ...stickyFocus } = useStickyHeadingFocus(0);
  return (
    <p {...stickyFocus} className={`sticky top-0 z-3 -mx-(--pane-inset) shrink-0 bg-panel-sidebar px-(--pane-inset) py-2 ${NOTE_CLASS} ${className}`}>{children}</p>
  );
}

function ProcessCategory({ title, note, children }: { title: string; note: string; children: React.ReactNode }): React.ReactElement {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="flex min-w-0 shrink-0 flex-col gap-2">
      <StickyHeading variant="category" id={headingId} note={note}>{title}</StickyHeading>
      {children}
    </section>
  );
}

function GroupedTrees({ trees, acting, onStop, onKeep }: {
  trees: ProcessTree[];
  acting: boolean;
  onStop: (tree: ProcessTree) => void;
  onKeep: (tree: ProcessTree) => void;
}): React.ReactElement {
  const t = useT();
  const groups = groupsOf(trees);
  const sessions = groups.filter((group) => group.sessionId !== undefined);
  const project = groups.find((group) => group.sessionId === undefined);
  return (
    <>
      {sessions.length > 0 && (
        <ProcessCategory title={t("processes.sessions")} note={t("processes.panelMessage")}>
          {sessions.map((group) => <TreeGroup key={group.key} group={group} acting={acting} onStop={onStop} onKeep={onKeep} />)}
        </ProcessCategory>
      )}
      {project && (
        <ProcessCategory title={t("processes.project")} note={t("processes.project.note")}>
          <TreeList trees={project.trees} acting={acting} keepable={false} onStop={onStop} onKeep={onKeep} />
        </ProcessCategory>
      )}
    </>
  );
}

/** A session's trees under its displayed name, within the Session processes category. */
function TreeGroup({ group, acting, onStop, onKeep }: {
  group: Group;
  acting: boolean;
  onStop: (tree: ProcessTree) => void;
  onKeep: (tree: ProcessTree) => void;
}): React.ReactElement {
  const t = useT();
  const headingId = useId();
  const session = useSession(group.sessionId);
  const heading = session ? sessionDisplayTitle(t, session) : t("processes.sessionUnknown");
  return (
    <section aria-labelledby={headingId} className="flex min-w-0 flex-col gap-2">
      <h4 id={headingId} className="text-sm font-medium">
        <FadeOverflow as="span" dir="auto" className="block" titleWhenClipped={heading}>{heading}</FadeOverflow>
      </h4>
      <TreeList trees={group.trees} acting={acting} keepable onStop={onStop} onKeep={onKeep} labelledBy={headingId} />
    </section>
  );
}

function TreeList({ trees, acting, keepable, onStop, onKeep, labelledBy }: {
  trees: ProcessTree[];
  acting: boolean;
  keepable: boolean;
  onStop: (tree: ProcessTree) => void;
  onKeep: (tree: ProcessTree) => void;
  labelledBy?: string;
}): React.ReactElement {
  const listRef = useListMotion<HTMLUListElement>();
  return (
    <ul ref={listRef} aria-labelledby={labelledBy} className="relative flex min-w-0 flex-col gap-2">
      {trees.map((tree) => (
        <li key={tree.id} data-list-motion={tree.id} className="min-w-0">
          <TreeRow tree={tree} acting={acting} keepable={keepable} onStop={() => onStop(tree)} onKeep={() => onKeep(tree)} />
        </li>
      ))}
    </ul>
  );
}

/** A tree: its command and details, and at the bottom end its Stop and, where `keepable`, Keep,
 * shown while the row is hovered or holds focus and always where nothing can hover. Each is named with the command so
 * the rows' buttons can be told apart. An open confirmation makes them unavailable. */
function TreeRow({ tree, acting, keepable, onStop, onKeep }: {
  tree: ProcessTree;
  acting: boolean;
  keepable: boolean;
  onStop: () => void;
  onKeep: () => void;
}): React.ReactElement {
  const t = useT();
  const command = isolate(tree.command === "" ? t("processes.commandUnknown") : tree.command);
  // In dark the Stop label is short of 4.5:1 on this card, a shortfall kept by the user's decision (see the
  // `--panel-row` comment in style.css and docs/ui-accessibility.md, "Conformance level and exceptions").
  return (
    <div className="group flex min-w-0 flex-col gap-1 rounded-lg bg-(--panel-row) px-3 py-2 shadow-(--panel-shadow)">
      <ProcessCommand tree={tree} />
      <ProcessTreeDetails tree={tree} actions={
        /* Clipping keeps hidden actions reachable with Tab without taking width from the facts;
           revealing them leaves the command and directory at the card's full width. */
        <div className={`${HOVER_ACTIONS_CLASS} items-center gap-1 [@media(hover:none)]:ms-2 group-hover:ms-2 group-focus-within:ms-2`}>
          {keepable && (
            <Button data-region-landing size="xs" variant="secondary" aria-label={t("processes.keepNamed", { command })} preventFocusOnPress isDisabled={acting} onPress={onKeep}>
              {t("processes.keep")}
            </Button>
          )}
          <Button data-region-landing={keepable ? undefined : true} size="xs" variant="danger-soft" aria-label={t("processes.stopNamed", { command })} preventFocusOnPress isDisabled={acting} onPress={onStop}>
            {t("processes.stop")}
          </Button>
        </div>
      } />
    </div>
  );
}
