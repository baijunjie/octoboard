import React, { createContext, useContext, useEffect, useState, useSyncExternalStore } from "react";
import { flushSync } from "react-dom";

import { useT } from "../../i18n/react";
import { focusAfterModalCloses } from "../focus/modalFocus";
import { useStableHandler } from "../hooks/useLatest";
import { ConfirmDialog } from "./ConfirmDialog";
import { createUnsavedChangesGuard, type GuardSnapshot, type UnsavedChangesGuard } from "./unsavedChangesGuard";

const GuardContext = createContext<UnsavedChangesGuard | undefined>(undefined);

/** Makes `guard` the one the forms and the ways of leaving below ask; the nearest provider wins. */
export function UnsavedChangesProvider({ guard, children }: { guard: UnsavedChangesGuard; children: React.ReactNode }): React.ReactElement {
  return <GuardContext.Provider value={guard}>{children}</GuardContext.Provider>;
}

/** Closes the question and runs the leave, the question first and inside `flushSync` with the discard, so that the
 * modal is gone before the leave moves focus and its own move is not pulled back; focus left on `<body>` goes to the
 * window's fallback. Outside the hook, which has no variable for the compiler to follow into a closure. */
function discardAndLeave(guard: UnsavedChangesGuard): void {
  let leave: (() => void) | undefined;
  flushSync(() => {
    leave = guard.answer(true);
  });
  leave?.();
  focusAfterModalCloses();
}

/**
 * The window's guard of changes not saved, and the question in front of leaving them. `run` is the way a leave goes
 * through the guard (`unsavedChangesGuard.ts`): it runs the leave at once when no form holds changes and otherwise opens
 * `confirmation`, a destructive confirmation (Keep editing, Discard) that the caller renders where no `!offline` gate
 * stands over it, since the Remote devices page works offline. The guard is made once and `run` never changes, so a
 * row that takes it renders again for nothing it holds; the owner renders again only when `dirty` or `asking` flips,
 * never for a keystroke. Keep editing, Escape and the close button leave the form where it was, and focus returns to
 * where it was; Discard gives up the changes of every form held and then runs the leave.
 */
export function useUnsavedChangesGuard(): {
  guard: UnsavedChangesGuard;
  run: UnsavedChangesGuard["run"];
  /** Some form holds changes that are not saved. */
  dirty: boolean;
  /** The question is open. */
  asking: boolean;
  confirmation: React.ReactElement | undefined;
} {
  const t = useT();
  const [guard] = useState(createUnsavedChangesGuard);
  const { dirty, asking }: GuardSnapshot = useSyncExternalStore(guard.subscribe, guard.getSnapshot);
  const confirmation = asking ? (
    <ConfirmDialog
      title={t("unsavedChanges.title")}
      message={t("unsavedChanges.message")}
      confirmLabel={t("unsavedChanges.discard")}
      cancelLabel={t("unsavedChanges.keep")}
      destructive
      // Discarding only gives up what is on screen, and the leave reports its own failure.
      errorTitle={t("unsavedChanges.failedTitle")}
      // Keep editing: the question may have come up as the surface focus would return to was closing (a quote from the
      // file viewer over a page), so focus left on `<body>` goes to the window's fallback, as for Discard.
      onCancel={() => {
        guard.answer(false);
        focusAfterModalCloses();
      }}
      onConfirm={async () => discardAndLeave(guard)}
    />
  ) : undefined;
  return { guard, run: guard.run, dirty, asking, confirmation };
}

/** The guard of the nearest provider, for what has to ask whether changes are held (a page that shows itself only
 * while none are); none outside one. */
export function useHeldGuard(): UnsavedChangesGuard | undefined {
  return useContext(GuardContext);
}

/**
 * Declares that the form its state owner holds has changes not saved (`unsaved`), for as long as it does. While it
 * does, every way of leaving that goes through the guard asks first; `saving` is whether a save is in flight, which
 * makes a leave pressed meanwhile do nothing; `discard` gives up the changes, as the form's Revert does, and is called
 * when the user answers Discard. Nothing outside a provider.
 */
export function useUnsavedChanges({ unsaved, saving, discard }: { unsaved: boolean; saving: boolean; discard: () => void }): void {
  const guard = useContext(GuardContext);
  const [key] = useState(() => ({}));
  const stableDiscard = useStableHandler(discard);
  useEffect(() => {
    if (!guard || !unsaved) return;
    guard.hold(key, { saving, discard: stableDiscard });
    return () => guard.release(key);
  }, [guard, key, unsaved]);
  // A save starting or ending replaces the held form in place: releasing it first would flip the guard's `dirty` twice.
  useEffect(() => {
    if (guard && unsaved) guard.hold(key, { saving, discard: stableDiscard });
  }, [saving]);
}

const LEAVE_FREELY = (leave: () => void): void => leave();

/** What a way of leaving goes through: the guard's `run` under a provider, and the leave itself with none. */
export function useLeaveGuard(): (leave: () => void) => void {
  const guard = useContext(GuardContext);
  return guard ? guard.run : LEAVE_FREELY;
}
