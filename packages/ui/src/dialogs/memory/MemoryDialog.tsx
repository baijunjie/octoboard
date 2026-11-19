import { Brain } from "lucide-react";
import React, { useEffect, useLayoutEffect, useRef, useState } from "react";

import { PaneFailure } from "../../components/alerts/PaneFailure";
import { raiseError } from "../../components/alerts/raiseError";
import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../../components/announce/StatusAnnouncer";
import { ConfirmDialog } from "../../components/dialog/ConfirmDialog";
import { Dialog } from "../../components/dialog/Dialog";
import { useFocusLeavingWhenDisabled } from "../../components/dialog/dialogHooks";
import { dialogAround } from "../../components/dialog/dialogElement";
import { afterFrames } from "../../components/focus/afterFrames";
import { useHandoffAfterRemoval } from "../../components/focus/HandoffOnUnmount";
import { errorPresentation } from "../../errors/errorPresentation";
import { ringForAutomaticFocus } from "../../input/lastInput";
import { EmptyPanel } from "../../components/panel/EmptyPanel";
import { LoadingState } from "../../components/panel/LoadingState";
import { InlineReference } from "../../i18n/InlineReference";
import { Message, useT } from "../../i18n/react";
import { isolate } from "../../i18n/isolate";
import { Button } from "../../primitives";
import { useDaemon, useDaemonStore } from "../../store";
import { MemoryFormDialog } from "./MemoryFormDialog";
import { MemoryRow } from "./MemoryRow";
import { MemoryViewDialog } from "./MemoryViewDialog";
import { MEMORY_MAX_ENTRIES } from "./memoryRules";
import { targetRequest, type MemoryTarget } from "./memoryTarget";
import { useMemory } from "./useMemory";

/** What the dialog has open over its list. */
type Open =
  | { kind: "view"; id: string; editing: boolean }
  | { kind: "edit"; id: string }
  | { kind: "add" }
  /** `summary` is what the confirmation names the entry by, taken when it was asked for. */
  | { kind: "delete"; id: string; summary: string }
  | { kind: "deleteAll" };

/** The words the dialog says that differ between a console's memory and a project's. */
const COPY = {
  console: {
    title: "memory.title",
    intro: "memory.intro",
    emptyDescription: "memory.empty.description",
    deleteMessage: "memory.delete.message",
    deleteAllSubject: "memory.deleteAll.subject",
    deleteAllMessage: "memory.deleteAll.message",
  },
  project: {
    title: "memory.project.title",
    intro: "memory.project.intro",
    emptyDescription: "memory.project.empty.description",
    deleteMessage: "memory.project.delete.message",
    deleteAllSubject: "memory.project.deleteAll.subject",
    deleteAllMessage: "memory.project.deleteAll.message",
  },
} as const;

/** A memory: a console's (the entries its console sessions have saved) or a project's (what every agent working in the
 * project keeps of what holds on the machine the project is on), which the user can read, correct, delete and add to. The list follows
 * the changes made while the dialog is open, and an entry's view or form opens over it. The dialog reads only the
 * daemon, so a window on a remote daemon has it too. */
export function MemoryDialog({ target, onClose }: { target: MemoryTarget; onClose: () => void }): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const copy = COPY[target.kind];
  // The name the console or project has now, which the user may have changed since opening the menu.
  const requested = target.kind === "console" ? target.console : target.project;
  const name = useDaemonStore((s) => (target.kind === "console" ? s.consoles : s.projects).get(requested.id)?.name) ?? requested.name;
  const { memory, status, failure, retry, retrying, attempt, read } = useMemory(target);
  const [open, setOpen] = useState<Open>();
  const addRef = useRef<HTMLButtonElement>(null);
  const deleteAllRef = useRef<HTMLButtonElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  // Each row's action menu button, the entry whose row last held focus (forgotten once focus moves to another control of
  // this dialog), the list as it stood before this render, and where focus goes once a deletion has settled: a row's
  // entry id, or "" for Add entry.
  const triggers = useRef(new Map<string, HTMLButtonElement>());
  const focusedEntry = useRef<string | undefined>(undefined);
  const shownBefore = useRef(memory.index);
  const handoff = useRef<string | undefined>(undefined);
  useEffect(() => {
    const forget = (event: FocusEvent) => {
      const target = event.target as Node;
      const dialog = dialogAround(addRef);
      // The dialog itself is no control: focus parked on it (a menu opened by a press that did not focus its button
      // closing, as WebKit leaves a pressed button unfocused) does not mean the user moved on from the row.
      if (dialog?.contains(target) && target !== dialog && !listRef.current?.contains(target)) focusedEntry.current = undefined;
    };
    document.addEventListener("focusin", forget);
    return () => document.removeEventListener("focusin", forget);
  }, []);
  // A deleted entry takes its row with it, and the focus its menu button held or would have got back: focus goes to
  // the row after it that is still listed, else the one before, else to Add entry. Only for the entry whose row held
  // focus, so a nested dialog closing over a row still listed leaves focus to React Aria's own restore.
  /** The entry whose row takes focus once the entry `gone` has left the list, or "" for Add entry. */
  const neighbourOf = (gone: string): string => {
    const listed = new Set(memory.index.map((line) => line.id));
    const before = shownBefore.current;
    const at = before.findIndex((line) => line.id === gone);
    return [...before.slice(at + 1), ...before.slice(0, Math.max(at, 0)).reverse()].find((line) => listed.has(line.id))?.id ?? "";
  };
  const destination = (id: string): HTMLElement | null => (id ? triggers.current.get(id) : undefined) ?? addRef.current;
  // A row removed while focus is in it (a session's deletion, with nothing open over the list) hands focus on in the
  // commit that removes it.
  const handoffFrom = useHandoffAfterRemoval(() => {
    const gone = focusedEntry.current;
    focusedEntry.current = undefined;
    return destination(gone === undefined ? "" : neighbourOf(gone));
  });
  // A row removed while focus is in a nested dialog (the deletion's confirmation) waits for it to close; declared after
  // the handoff above, which has then already taken the entry it handed on from.
  useLayoutEffect(() => {
    const gone = focusedEntry.current;
    if (gone !== undefined && !memory.index.some((line) => line.id === gone)) {
      handoff.current = neighbourOf(gone);
      focusedEntry.current = undefined;
    }
    shownBefore.current = memory.index;
  }, [memory.index]);
  // Once the nested dialog has closed, the handoff waits a frame past React Aria's restore, which finds the row it
  // would return to gone and puts focus on <body> or on a control of the dialog of its own choosing; it overrides both.
  useEffect(() => {
    if (handoff.current === undefined || open !== undefined) return;
    return afterFrames(1, () => {
      const target = destination(handoff.current ?? "");
      handoff.current = undefined;
      const active = document.activeElement;
      if (!target || !(active === document.body || active === null || dialogAround(addRef)?.contains(active))) return;
      ringForAutomaticFocus();
      target.focus();
    });
  }, [memory.index, open]);
  // Delete all turns unavailable as the last entry goes, which can happen while it holds focus.
  useFocusLeavingWhenDisabled(deleteAllRef, memory.index.length === 0, addRef, () => dialogAround(addRef));
  // The entries are read before their dialog opens, so the dialog has its content from the first frame.
  const openEntry = (id: string, next: Open) => {
    read(id).then(
      () => setOpen(next),
      (error: unknown) => raiseError(error, { title: t("memory.openFailedTitle") }),
    );
  };
  const full = memory.index.length >= MEMORY_MAX_ENTRIES;
  const empty = status === "ready" && memory.index.length === 0;
  const shownName = isolate(name);

  return (
    <>
      <Dialog
        title={t(copy.title)}
        onClose={onClose}
        size="lg"
        footerStart={
          status === "ready" && memory.index.length > 0 && (
            // The plural follows the limit, which the noun goes with ("1 of 200 entries").
            <p className="text-sm text-app-muted tabular-nums">{t("memory.count", { used: memory.index.length, count: MEMORY_MAX_ENTRIES })}</p>
          )
        }
        footer={
          <>
            <Button ref={deleteAllRef} type="button" variant="danger-soft" disabled={memory.index.length === 0} onPress={() => setOpen({ kind: "deleteAll" })}>
              {t("memory.deleteAll")}
            </Button>
            <Button
              ref={addRef}
              type="button"
              disabled={status !== "ready"}
              unavailable={full ? t("memory.fullReason", { limit: MEMORY_MAX_ENTRIES }) : ""}
              onPress={() => setOpen({ kind: "add" })}
            >
              {t("memory.add")}
            </Button>
          </>
        }
      >
        <StatusAnnouncer text={status === "loading" ? t("memory.loading") : undefined} firstWriteDelayMs={AFTER_FOCUS_MOVE_DELAY_MS} />
        <p className="text-sm text-app-muted">
          <Message id={copy.intro} params={{ name: <InlineReference dir="auto" wrap>{name}</InlineReference> }} />
        </p>
        {status === "loading" && <LoadingState label={t("memory.loading")} />}
        {status === "failed" && (
          <PaneFailure title={t("memory.loadFailed.title")} message={errorPresentation(failure)} announce attempt={attempt} pending={retrying} onRetry={retry} />
        )}
        {empty && (
          <EmptyPanel
            icon={Brain}
            message={t("memory.empty.title")}
            description={t(copy.emptyDescription)}
          />
        )}
        {status === "ready" && memory.index.length > 0 && (
          <ul ref={listRef} aria-label={t("memory.list", { name: shownName })} className="flex flex-col [&>li]:hairline-b [&>li:last-child]:border-b-0">
            {memory.index.map((line) => (
              <React.Fragment key={line.id}>
                {handoffFrom(`[data-memory-entry="${line.id}"]`)}
                <MemoryRow
                  line={line}
                  actionsRef={(button) => {
                    if (button) triggers.current.set(line.id, button);
                    else triggers.current.delete(line.id);
                  }}
                  onFocus={() => (focusedEntry.current = line.id)}
                  flagged={target.kind === "console"}
                  onView={() => openEntry(line.id, { kind: "view", id: line.id, editing: false })}
                  onEdit={() => openEntry(line.id, { kind: "edit", id: line.id })}
                  onDelete={() => {
                    // A press with a mouse need not focus the row (WebKit leaves a pressed button unfocused), so the row
                    // whose entry is being deleted is noted here too, for the handoff once it has gone.
                    focusedEntry.current = line.id;
                    setOpen({ kind: "delete", id: line.id, summary: line.summary });
                  }}
                />
              </React.Fragment>
            ))}
          </ul>
        )}
      </Dialog>
      {open?.kind === "view" && (
        <MemoryViewDialog
          id={open.id}
          memory={memory}
          flagged={target.kind === "console"}
          onEdit={() => setOpen({ ...open, editing: true })}
          onClose={() => setOpen(undefined)}
        />
      )}
      {((open?.kind === "view" && open.editing) || open?.kind === "edit") && (
        <MemoryFormDialog
          target={target}
          editing={open.id}
          memory={memory}
          reread={() => read(open.id, true)}
          onClose={() => setOpen(open.kind === "view" ? { ...open, editing: false } : undefined)}
        />
      )}
      {open?.kind === "add" && <MemoryFormDialog target={target} memory={memory} onClose={() => setOpen(undefined)} />}
      {open?.kind === "delete" && (
        <ConfirmDialog
          title={t("memory.delete.title")}
          errorTitle={t("memory.delete.failedTitle")}
          subject={<Message id="memory.delete.subject" params={{ summary: <InlineReference dir="auto" wrap>{open.summary}</InlineReference> }} />}
          message={t(copy.deleteMessage)}
          confirmLabel={t("common.delete")}
          destructive
          onCancel={() => setOpen(undefined)}
          onConfirm={async () => {
            await request({ type: "delete_memory", ...targetRequest(target), memory: open.id });
            setOpen(undefined);
          }}
        />
      )}
      {open?.kind === "deleteAll" && (
        <ConfirmDialog
          title={t("memory.deleteAll.title")}
          errorTitle={t("memory.deleteAll.failedTitle")}
          subject={<Message id={copy.deleteAllSubject} params={{ name: <InlineReference dir="auto" wrap>{name}</InlineReference> }} />}
          message={t(copy.deleteAllMessage)}
          confirmLabel={t("memory.deleteAll")}
          typeToConfirm={t("dialog.typeToConfirm.delete")}
          destructive
          onCancel={() => setOpen(undefined)}
          onConfirm={async () => {
            await request({ type: "delete_memories", ...targetRequest(target) });
            // Delete all, which the confirmation would return focus to, is unavailable now: focus goes to Add entry.
            handoff.current = "";
            setOpen(undefined);
          }}
        />
      )}
    </>
  );
}
