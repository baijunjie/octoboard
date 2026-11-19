import { useEffect, useRef, useState } from "react";

/**
 * Which fields of a page form say what is wrong with them (`docs/conventions/ui-components.md`, "Dialogs and fields"):
 * a field that was edited and then left, and every field from a Save pressed with something invalid. A page form stays
 * open while the user works through it, so a field is never marked merely because another control changed.
 *
 * `edit` names the fields a change touched, `leave` is a field's blur (it marks the field only if it was edited),
 * `markLeft` marks one that has no blur of its own or was filled in as good as edited and left (a pasted text, a
 * checkbox), `shown` passes a field's message on once it is marked, `attempt` is a Save that found something invalid
 * (it marks every field and counts, which `useFocusFirstInvalid` moves focus on), and `forget` drops every mark with the
 * draft they belong to.
 */
export function useValidationMarks() {
  // Read only as a field is left, never drawn, and a ref so that a field's own change and its leaving in one event (a
  // date field emptied as it is left) are seen in order.
  const edited = useRef<ReadonlySet<string>>(new Set());
  const [left, setLeft] = useState<ReadonlySet<string>>(new Set());
  const [attempted, setAttempted] = useState(false);
  const [attempts, setAttempts] = useState(0);
  const markLeft = (field: string) => setLeft((current) => (current.has(field) ? current : new Set([...current, field])));
  return {
    edit: (fields: readonly string[]) => {
      edited.current = new Set([...edited.current, ...fields]);
    },
    leave: (field: string) => {
      if (edited.current.has(field)) markLeft(field);
    },
    markLeft,
    shown: (field: string, message: string | undefined) => (attempted || left.has(field) ? message : undefined),
    attempt: () => {
      setAttempted(true);
      setAttempts((count) => count + 1);
    },
    /** How many Saves found something invalid, for `useFocusFirstInvalid`. */
    attempts,
    forget: () => {
      edited.current = new Set();
      setLeft(new Set());
      setAttempted(false);
    },
  };
}

/** A Save that found something wrong (`attempts` grew) moves focus to the first invalid field of the form, once the
 * marks have been drawn. */
export function useFocusFirstInvalid(form: React.RefObject<HTMLFormElement | null>, attempts: number): void {
  useEffect(() => {
    if (attempts > 0) form.current?.querySelector<HTMLElement>("[aria-invalid=true]")?.focus();
  }, [attempts]);
}
