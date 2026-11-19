/** A form that holds changes not saved yet, as the guard knows it. */
export interface HeldForm {
  /** A save is in flight: the form stays held, and a way of leaving pressed meanwhile does nothing until it is answered. */
  saving: boolean;
  /** Gives up the changes, as the form's own Revert does. */
  discard: () => void;
}

/** What `run` did with a way of leaving: ran it at once, asked first, or dropped the press. */
export type LeaveOutcome = "left" | "asked" | "held";

/** What a render reads of the guard, a new object only when one of the two changes. */
export interface GuardSnapshot {
  /** Some form holds changes that are not saved. */
  readonly dirty: boolean;
  /** The question whether to discard them is open. */
  readonly asking: boolean;
}

export interface UnsavedChangesGuard {
  /** Registers `form` under `key`, or replaces what the key registered. */
  hold(key: object, form: HeldForm): void;
  /** Takes the key's form out; when that leaves none held while the question is open, the question closes and the
   * leave waiting on it is dropped, as a confirmation whose subject has gone closes without a word. */
  release(key: object): void;
  /** Runs `leave` at once when no form is held. While a held form is saving, drops the press. Otherwise opens the
   * question and keeps `leave` for its answer; a second `run` while it is open replaces the leave waiting. */
  run(leave: () => void): LeaveOutcome;
  isDirty(): boolean;
  /** Answers the open question. Discarding calls every held form's `discard`, empties the registry and returns the leave
   * to run, so that anything it calls guarded runs at once; keeping only closes the question. Nothing while none is open. */
  answer(discard: boolean): (() => void) | undefined;
  /** Runs `hook` as the question opens (not for a `run` that replaces the leave of one already open), so that what
   * the question is raised over can be remembered; returns what removes it. */
  setBeforeAsk(hook: () => void): () => void;
  subscribe(listener: () => void): () => void;
  /** For `useSyncExternalStore`: the same object until `dirty` or `asking` changes. */
  getSnapshot(): GuardSnapshot;
}

/**
 * The window's registry of forms with changes not saved, and the one question that stands before every way of leaving
 * one: a way of leaving goes through `run`, which asks first while a form is held (`UnsavedChanges.tsx` draws the
 * question). It has no React in it, and is shaped as `viewer/image/zoomGroup.ts` is, for `useSyncExternalStore`.
 */
export function createUnsavedChangesGuard(): UnsavedChangesGuard {
  const forms = new Map<object, HeldForm>();
  const listeners = new Set<() => void>();
  let pending: (() => void) | undefined;
  let asking = false;
  let beforeAsk: (() => void) | undefined;
  let snapshot: GuardSnapshot = { dirty: false, asking: false };

  const changed = () => {
    const dirty = forms.size > 0;
    if (dirty === snapshot.dirty && asking === snapshot.asking) return;
    snapshot = { dirty, asking };
    for (const listener of listeners) listener();
  };

  return {
    hold(key, form) {
      forms.set(key, form);
      changed();
    },
    release(key) {
      if (!forms.delete(key)) return;
      if (forms.size === 0) {
        asking = false;
        pending = undefined;
      }
      changed();
    },
    run(leave) {
      if (forms.size === 0) {
        leave();
        return "left";
      }
      if (Array.from(forms.values()).some((form) => form.saving)) return "held";
      pending = leave;
      if (!asking) {
        beforeAsk?.();
        asking = true;
        changed();
      }
      return "asked";
    },
    isDirty: () => forms.size > 0,
    answer(discard) {
      if (!asking) return undefined;
      const leave = pending;
      pending = undefined;
      asking = false;
      if (!discard) {
        changed();
        return undefined;
      }
      const held = Array.from(forms.values());
      forms.clear();
      for (const form of held) form.discard();
      changed();
      return leave;
    },
    setBeforeAsk(hook) {
      beforeAsk = hook;
      return () => {
        if (beforeAsk === hook) beforeAsk = undefined;
      };
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    getSnapshot: () => snapshot,
  };
}
