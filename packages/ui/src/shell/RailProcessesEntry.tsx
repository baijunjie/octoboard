import { SquareActivity } from "lucide-react";
import React, { useEffect, useRef } from "react";

import { HandOffOnUnmount } from "../components/focus/HandOffOnUnmount";
import { useT } from "../i18n/react";
import type { ErrorPresentation } from "../errors/errorPresentation";
import type { ProcessJump } from "../process/processJump";
import { useAllProcesses } from "../process/useAllProcesses";
import type { ProcessCount } from "../protocol";
import { sessionDisplayTitle } from "../sessions/sessionLabel";
import { useDaemonStoreEqual } from "../store";
import { CountedGlyph } from "./CountedGlyph";
import { nestedPopovers } from "./nestedPopover";
import { PANEL_ATTRIBUTE, ProcessCounts, type ProcessRow } from "./ProcessCounts";
import { RailPanelEntry } from "./RailPanelEntry";

const TRIGGER_ID = "rail-processes";
const OWNER = "rail-processes";

const nested = nestedPopovers(OWNER);

/** What holds the focus, if it is in `selector` or in a popover opened from the panel. */
const focusHolder = (selector: string): Element | null => document.activeElement?.closest(selector) ?? nested.holding(document.activeElement);

/** The most frames to wait for the popover to leave the document before going on without it. */
const LEAVE_FRAMES = 60;

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
 * faster while the popover is open. The store is read only while something is running, and then only the projects
 * and sessions the counts name. */
export function RailProcessesEntry({ onOpen }: { onOpen: (target: ProcessJump) => boolean }): React.ReactElement | null {
  const [open, setOpen] = React.useState(false);
  const { counts, error } = useAllProcesses(open ? "live" : "background");
  const running = counts !== undefined && counts.length > 0;
  // The popover goes with the last process, without telling it is closed.
  if (!running && open) setOpen(false);
  if (!running) return null;
  return <RunningProcesses counts={counts} error={error} onOpenChange={setOpen} onOpen={onOpen} />;
}

/** The entry while some process runs: the button, the popover and the rows `counts` name. */
function RunningProcesses({ counts, error, onOpenChange, onOpen }: {
  counts: ProcessCount[];
  error: ErrorPresentation | undefined;
  onOpenChange: (open: boolean) => void;
  onOpen: (target: ProcessJump) => boolean;
}): React.ReactElement | null {
  const t = useT();
  // The rows as drawn, built from only the records the counts name and the names of their consoles, so an event
  // that changes nothing a row shows renders nothing here.
  const rows = useDaemonStoreEqual((s) => counts.flatMap((entry): ProcessRow[] => {
    if (entry.project_id !== undefined) {
      const project = s.projects.get(entry.project_id);
      return project ? [{ key: `project:${project.id}`, title: project.name, context: s.consoles.get(project.console_id)?.name, count: entry.trees, target: { project: project.id } }] : [];
    }
    if (entry.session_id !== undefined) {
      const session = s.sessions.get(entry.session_id);
      return session ? [{ key: `session:${session.id}`, title: sessionDisplayTitle(t, session), context: s.consoles.get(session.console_id)?.name, count: entry.trees, target: { consoleSession: session.id } }] : [];
    }
    return [];
  }));
  const trigger = useRef<HTMLButtonElement>(null);
  // Set by a press of a row: the jump that follows lands focus in the Processes panel itself, so the panel's closing
  // hands it nowhere (it would put the keyboard ring on the rail button, and the panel after it, for a mouse click).
  const jumping = useRef(false);
  const count = rows.reduce((sum, row) => sum + row.count, 0);
  // Every project and session the counts name may be gone from the UI: the popover goes with them.
  useEffect(() => {
    if (count === 0) onOpenChange(false);
  }, [count]);
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
      <RailPanelEntry
        owner={OWNER}
        label={t("panel.processes.name")}
        triggerId={TRIGGER_ID}
        triggerLabel={t("rail.processes", { count })}
        triggerRef={trigger}
        glyph={
          <CountedGlyph count={count} overflowLabel={t("rail.countBadgeOverflow")} color="accent">
            <SquareActivity aria-hidden="true" className="size-4" />
          </CountedGlyph>
        }
        onOpenChange={(isOpen) => {
          onOpenChange(isOpen);
          if (isOpen) jumping.current = false;
        }}
      >
        {({ close, form, popupId }) => (
          <>
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
          </>
        )}
      </RailPanelEntry>
    </>
  );
}
