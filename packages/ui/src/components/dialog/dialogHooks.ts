import React, { useEffect, useLayoutEffect, useRef, useState } from "react";

import { useLatest } from "../hooks/useLatest";
import { settle } from "../hooks/settle";
import { raiseError } from "../alerts/raiseError";
import type { ErrorRecovery } from "../../errors/errorRecovery";
import { DISABLED_CONTROL } from "../focus/selectors";

/** The busy state every dialog that talks to the daemon needs. Execution failures go to the acknowledgement queue; what
 * is wrong with a single field goes to that field's own `errorMessage` instead (see `useSubmitValidation`). `run`
 * marks the dialog busy for the duration of `action` and reports a rejection to the queue instead of letting it
 * escape, so the dialog stays open for another try. A dialog reused for successive subjects passes a `resetKey` naming
 * the current one: when it changes the busy state is cleared, and an action still running for the previous subject no
 * longer reports into it. A failure is titled `errorTitle`, the plain name of what failed; `errorRecovery` gives the
 * remedy for a failure where the action knows a better one than its kind has (see `NotificationRequest.recovery`). */
export function useDialogAction(
  resetKey: string | undefined,
  errorTitle: string,
  errorRecovery?: (error: unknown) => ErrorRecovery | undefined,
): {
  busy: boolean;
  run: (action: () => Promise<void>) => Promise<void>;
} {
  const [busy, setBusy] = useState(false);
  const [seenKey, setSeenKey] = useState(resetKey);
  const currentKey = useLatest(resetKey);
  if (seenKey !== resetKey) {
    setSeenKey(resetKey);
    setBusy(false);
  }

  const run = (action: () => Promise<void>) => {
    const startedFor = resetKey;
    const stale = () => currentKey.current !== startedFor;
    setBusy(true);
    return settle(
      action,
      (err) => {
        if (!stale()) raiseError(err, { title: errorTitle, keepTitle: true, recovery: errorRecovery?.(err) });
      },
      () => {
        if (!stale()) setBusy(false);
      },
    );
  };

  return { busy, run };
}

/** Puts keyboard focus back on the element `target` returns when `deps` change and focus has fallen to `<body>`,
 * which is where it goes when the focused control is unmounted by the change. Left alone, a dialog's
 * Escape handling and Tab containment stop working, since they hang off focus being inside it.
 *
 * Never at mount, StrictMode's second run included: a dialog opened by a press without a pointer (assistive
 * technology, a script) has no focus yet then, since react-aria puts off its opening focus (`autoFocus`, or the dialog
 * itself) until transitions end and drops it if focus has moved meanwhile, so focusing anything here would take the
 * dialog's opening focus away from its first field. */
export function useRefocusIfLost(target: () => HTMLElement | null | undefined, deps: React.DependencyList): void {
  const seen = useRef(deps);
  useEffect(() => {
    const changed = deps.some((dep, index) => !Object.is(dep, seen.current[index]));
    seen.current = deps;
    if (changed && document.activeElement === document.body) target()?.focus();
  }, deps);
}

/** Moves keyboard focus from `button` to `fallback` when `disabled` turns true while the button holds it: a natively
 * disabled button cannot keep focus, and the dialog's Escape and Tab containment hang off focus being inside it. WebKit
 * blurs the button to `<body>` as it becomes disabled, before this runs, so the button counts as having held focus when
 * `<body>` has it and the button had it last. Where the fallback is disabled too (a busy dialog disables everything but
 * its pending button), focus goes to the dialog instead. A layout effect, so focus has moved before the change is
 * painted. */
export function useFocusLeavingWhenDisabled(
  button: React.RefObject<HTMLElement | null>,
  disabled: boolean,
  fallback: React.RefObject<HTMLElement | null>,
  dialog: () => HTMLElement | null | undefined,
): void {
  const held = useRef(false);
  useLayoutEffect(() => {
    held.current = document.activeElement === button.current;
    const track = (event: FocusEvent) => {
      if (event.target !== button.current) return;
      // A blur that arrives once the button is disabled is the browser's doing, not the user moving on.
      if (event.type === "focusin") held.current = true;
      else if (!(event.target as HTMLButtonElement).disabled) held.current = false;
    };
    document.addEventListener("focusin", track);
    document.addEventListener("focusout", track);
    return () => {
      document.removeEventListener("focusin", track);
      document.removeEventListener("focusout", track);
    };
  }, []);
  useLayoutEffect(() => {
    if (!disabled) return;
    const active = document.activeElement;
    if (button.current && (active === button.current || ((active === document.body || !active) && held.current))) {
      const target = fallback.current;
      (target && !target.matches(DISABLED_CONTROL) ? target : dialog())?.focus();
    }
    held.current = false;
  }, [disabled]);
}

/** Holds back what a dialog's own checks found until the user has tried to submit, so a field is
 * not marked before it has been filled in. From then on the message follows what is typed, and
 * correcting the field takes it away. */
export function useSubmitValidation(): {
  /** `message` once a submit has found something wanting, nothing before that. */
  shown: <T extends string>(message: T | undefined) => T | undefined;
  /** Records the attempt and answers whether the dialog may go ahead with it. */
  attempt: (...messages: (string | undefined)[]) => boolean;
} {
  const [attempted, setAttempted] = useState(false);
  return {
    shown: (message) => (attempted ? message : undefined),
    attempt: (...messages) => {
      setAttempted(true);
      return messages.every((message) => message === undefined);
    },
  };
}
