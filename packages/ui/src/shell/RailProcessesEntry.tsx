import { ChevronRight, SquareActivity } from "lucide-react";
import React, { useEffect, useMemo, useRef } from "react";
import { mergeRefs } from "react-aria";

import { ErrorAlert } from "../components/alerts/ErrorAlert";
import { errorDetails } from "../components/alerts/errorParts";
import { Button } from "../components/controls/Button";
import { HandOffOnUnmount } from "../components/focus/HandOffOnUnmount";
import { Hint } from "../components/hint/Hint";
import { PopoverOwnerContext } from "../components/hint/popoverOwner";
import { CountBadge } from "../components/status/CountBadge";
import { useHasPainted } from "../components/motion/useHasPainted";
import { useListMotion } from "../components/motion/useListMotion";
import { FadedScroller } from "../components/scroll/scrollFade";
import { FadeOverflow } from "../components/text/FadeOverflow";
import type { ErrorPresentation } from "../errors/errorPresentation";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import type { ProcessJump } from "../process/processJump";
import { useAllProcesses } from "../process/useAllProcesses";
import { sessionDisplayTitle } from "../sessions/sessionLabel";
import { useDaemonStore } from "../store";
import { ChromeButton } from "./ChromeButton";
import { CloseWhenCovered } from "./CloseWhenCovered";
import { CountedGlyph } from "./CountedGlyph";
import { nestedPopovers } from "./nestedPopover";

const TRIGGER_ID = "rail-processes";
const PANEL_ATTRIBUTE = "data-rail-processes-panel";
const OWNER = "rail-processes";

const nested = nestedPopovers(OWNER);

/** What holds the focus, if it is in `selector` or in a popover opened from the panel. */
const focusHolder = (selector: string): Element | null => document.activeElement?.closest(selector) ?? nested.holding(document.activeElement);

/** The most frames to wait for the popover to leave the document before going on without it. */
const LEAVE_FRAMES = 60;

/** One line of the panel: a project or a console session with running trees, and where its Processes panel is. */
interface ProcessRow {
  key: string;
  title: string;
  /** What the title sits in: the console of the project or console session. */
  context?: string;
  count: number;
  target: ProcessJump;
}

/** Runs `then` once the element `id` is out of the document (or after `LEAVE_FRAMES` frames): a popover that is
 * closing gives focus back to its trigger as it leaves, which would override a focus move made before that. */
function whenRemoved(id: string, then: () => void): void {
  let frames = 0;
  const check = () => {
    if (document.getElementById(id) === null || ++frames > LEAVE_FRAMES) then();
    else requestAnimationFrame(check);
  };
  requestAnimationFrame(check);
}

/** The rail's button for the processes running across every project, with the count of their trees, and the popover
 * it anchors: one row per project and per console session with running trees, each the way to the Processes panel
 * that lists them. The popover lists no process itself. A project the UI does not know is left out, count included,
 * so the button and the rows agree. Not shown while nothing is running. The count is asked for now and then, and
 * faster while the popover is open. */
export function RailProcessesEntry({ onOpen }: { onOpen: (target: ProcessJump) => boolean }): React.ReactElement | null {
  const t = useT();
  const [open, setOpen] = React.useState(false);
  const { counts, error } = useAllProcesses(open ? "live" : "background");
  const projects = useDaemonStore((s) => s.projects);
  const consoles = useDaemonStore((s) => s.consoles);
  const sessions = useDaemonStore((s) => s.sessions);
  const trigger = useRef<HTMLButtonElement>(null);
  // Set by a press of a row: the jump that follows lands focus in the Processes panel itself, so the panel's closing
  // hands it nowhere (it would put the keyboard ring on the rail button, and the panel after it, for a mouse click).
  const jumping = useRef(false);
  const rows = useMemo(() => {
    const result: ProcessRow[] = [];
    for (const entry of counts ?? []) {
      if (entry.project_id !== undefined) {
        const project = projects.get(entry.project_id);
        if (project) result.push({ key: `project:${project.id}`, title: project.name, context: consoles.get(project.console_id)?.name, count: entry.trees, target: { project: project.id } });
      } else if (entry.session_id !== undefined) {
        const session = sessions.get(entry.session_id);
        if (session) result.push({ key: `session:${session.id}`, title: sessionDisplayTitle(t, session), context: consoles.get(session.console_id)?.name, count: entry.trees, target: { consoleSession: session.id } });
      }
    }
    return result;
  }, [counts, projects, consoles, sessions, t]);
  const count = rows.reduce((sum, row) => sum + row.count, 0);
  // The popover goes with the last process, without telling it is closed.
  if (count === 0 && open) setOpen(false);
  if (count === 0) return null;
  // The control after this one on the rail, or before it when this is the last.
  const handOff = () => {
    const button = trigger.current;
    const controls = Array.from(button?.closest("nav")?.querySelectorAll<HTMLElement>("button:not(:disabled)") ?? []);
    const at = button ? controls.indexOf(button) : -1;
    (controls[at + 1] ?? controls[at - 1])?.focus();
  };
  // Deferred hand-offs land on the rail button only if focus is still unclaimed by then (a dialog or another control
  // may have taken it since).
  const focusTriggerIfUnclaimed = () => {
    if (!trigger.current?.isConnected || (document.activeElement && document.activeElement !== document.body)) return;
    trigger.current.focus();
  };
  return (
    <>
      <HandOffOnUnmount from={() => focusHolder(`#${TRIGGER_ID}, [${PANEL_ATTRIBUTE}]`)} to={handOff} />
      <Hint
        kind="popover"
        label={t("panel.processes.name")}
        placement="end bottom"
        offset={8}
        contentClassName="w-96 max-w-[calc(100vw-4rem)] p-0 text-sm"
        // The Details popover of the refresh failure is portalled out of the popover but belongs to it: a press in it is
        // not an outside press, and Escape in it closes it first.
        isInside={(target) => nested.holding(target) !== null}
        yieldsEscape={(event) => nested.holding(event.target) !== null}
        // Opening the Details popover makes the rest of the page inert, which the browser answers with a pointer leave on
        // this popover: with the pointer still, that must not close this popover and take the Details popover with it.
        holdsOpen={() => nested.opened().length > 0}
        onOpenChange={(isOpen) => {
          setOpen(isOpen);
          if (isOpen) jumping.current = false;
        }}
        trigger={({ ref, onHoverStart: _start, onHoverEnd: _end, preventFocusOnPress: _prevent, ...wiring }) => (
          <ChromeButton id={TRIGGER_ID} label={t("rail.processes", { count })} buttonRef={mergeRefs(ref, trigger)} noTooltip {...wiring}>
            <CountedGlyph count={count} overflowLabel={t("rail.countBadgeOverflow")} color="accent">
              <SquareActivity aria-hidden="true" className="size-4" />
            </CountedGlyph>
          </ChromeButton>
        )}
      >
        {({ close, form, popupId }) => (
          <PopoverOwnerContext.Provider value={OWNER}>
            <CloseWhenCovered close={close} own={popupId} owner={OWNER} />
            {/* A popover that closes with focus inside it (a click on Details moved it there) would let it fall to the body.
                Not for a row's press: the jump lands focus itself. Focus moves once React's commit is over, or react-aria would
                not see it as the keyboard's and show no ring. */}
            <HandOffOnUnmount from={() => jumping.current ? null : focusHolder(`[${PANEL_ATTRIBUTE}]`)} to={() => queueMicrotask(focusTriggerIfUnclaimed)} />
            <ProcessCounts
              rows={rows}
              error={error}
              // A press opens a popover outside the rail's tab order, so focus is brought to its first row; one a mouse
              // rests on leaves focus where it was.
              focusFirst={form === "press"}
              onPress={(target) => {
                jumping.current = true;
                close();
                whenRemoved(popupId, () => {
                  // A row whose project or session is gone since lands nowhere: the rail button takes the focus.
                  if (!onOpen(target)) focusTriggerIfUnclaimed();
                });
              }}
            />
          </PopoverOwnerContext.Provider>
        )}
      </Hint>
    </>
  );
}

/** The popover's contents: a failed refresh, then a row for each of `rows`. */
function ProcessCounts({ rows, error, focusFirst, onPress }: {
  rows: ProcessRow[];
  error: ErrorPresentation | undefined;
  focusFirst: boolean;
  onPress: (target: ProcessJump) => void;
}): React.ReactElement {
  const t = useT();
  const root = useRef<HTMLDivElement>(null);
  // A failure already there as the popover opens is part of what it opened with; one that arrives while it is open is
  // announced.
  const hasPainted = useHasPainted();
  const listRef = useListMotion<HTMLUListElement>({ fitHeight: true });
  useEffect(() => {
    if (focusFirst) root.current?.querySelector<HTMLElement>("ul button")?.focus();
  }, []);
  return (
    <FadedScroller ref={root} {...{ [PANEL_ATTRIBUTE]: "" }} className="flex max-h-[70vh] flex-col gap-1 p-2">
      {error !== undefined && (
        <ErrorAlert status="warning" title={t("processes.refreshFailedTitle")} details={errorDetails(error)} announce={hasPainted}>
          {t("processes.global.refreshFailed")}
        </ErrorAlert>
      )}
      <ul ref={listRef} className="relative flex min-w-0 flex-col gap-1">
        {rows.map((row) => (
          <li key={row.key} data-list-motion={row.key} className="min-w-0">
            <Button variant="ghost" size="sm" className="h-auto min-h-0 w-full justify-start gap-2 rounded-lg px-2 py-1.5 font-normal"
              aria-label={row.context === undefined
                ? t("processes.global.show", { name: isolate(row.title), count: row.count })
                : t("processes.global.showIn", { name: isolate(row.title), console: isolate(row.context), count: row.count })}
              onPress={() => onPress(row.target)}>
              <span className="flex min-w-0 flex-1 flex-col text-start">
                <FadeOverflow as="span" dir="auto" className="block font-semibold" titleWhenClipped={row.title}>{row.title}</FadeOverflow>
                {row.context !== undefined && <FadeOverflow as="span" dir="auto" className="block text-xs text-app-muted" titleWhenClipped={row.context}>{row.context}</FadeOverflow>}
              </span>
              <CountBadge count={row.count} color="accent" className="tabular-nums" />
              <ChevronRight aria-hidden="true" className="-me-0.5 size-3.5 shrink-0 text-app-muted rtl:-scale-x-100" />
            </Button>
          </li>
        ))}
      </ul>
    </FadedScroller>
  );
}
