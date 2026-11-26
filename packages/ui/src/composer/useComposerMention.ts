import { useId, useState, type RefObject } from "react";

import { useLatest, useStableHandler } from "../components/hooks/useLatest";
import { isImeKey } from "../input/imeKey";
import type { Session } from "../protocol";
import { useMentionOptions, type MentionOption } from "./MentionPicker";
import type { MentionPaths } from "./useMentionPaths";
import { applyMention, mentionAt, type MentionToken } from "./mentions";

/** Where `token`'s `@` is, and what follows it; none for no mention. Kept outside the hook, whose compiler cannot lower
 * optional chaining in a logical test. */
const startOf = (token: MentionToken | undefined): number | undefined => token?.start;
const queryOf = (token: MentionToken | undefined): string | undefined => token?.query;

/** Whether two mentions (or none) are the same. */
function sameMention(a: MentionToken | undefined, b: MentionToken | undefined): boolean {
  if (a === undefined || b === undefined) return a === b;
  return a.start === b.start && a.end === b.end && a.query === b.query;
}

export interface ComposerMention {
  /** Whether the picker shows: a mention is being typed, Escape has not closed it, and nothing is being sent. */
  pickerOpen: boolean;
  /** The id the picker's list takes, which the text area names while the picker shows. */
  listId: string;
  options: readonly MentionOption[];
  files: MentionPaths["state"] | undefined;
  /** The option the keys or the pointer have moved to. */
  activeIndex: number;
  setActive: (index: number) => void;
  /** Reads the mention at the caret again. Called from the field's `onChange` for an edit, in the same React event as
   * the text's own update (called from a listener of the field's own, its state update would render before React had
   * read the edit, and the controlled value would be put back over what was typed), and on a move of the caret alone. */
  sync: () => void;
  /** Takes a key pressed in the field while the picker shows: the arrow keys move through its options, Enter and Tab
   * pick the one moved to, Escape closes it; a key an input method is composing with is the method's. Returns whether
   * it took the key, which the caller then keeps from doing anything else. Stable, for a listener set up once. */
  onKey: (event: KeyboardEvent) => boolean;
  /** Puts the option at `index` in place of the mention being typed. Stable. */
  pick: (index: number) => void;
  /** Forgets the mention, as focus leaves the field. Stable. */
  close: () => void;
}

/**
 * The `@` mention being typed in `session`'s composer and its picker: the mention at the caret of the text area behind
 * `fieldRef`, the one Escape closed (by where its `@` is, until another is begun), the options offered for it and the
 * one moved to. A picked option goes through `apply`, with the text as it then reads and where the caret goes.
 */
export function useComposerMention({ session, fieldRef, sending, apply }: {
  session: Session;
  fieldRef: RefObject<HTMLTextAreaElement | null>;
  sending: boolean;
  apply: (text: string, caret: number) => void;
}): ComposerMention {
  const [mention, setMention] = useState<MentionToken>();
  const [dismissed, setDismissed] = useState<number | null>(null);
  const [activeOption, setActiveOption] = useState(0);
  const pickerOpen = mention !== undefined && mention.start !== dismissed && !sending;
  const { options, files } = useMentionOptions(session, mention?.query ?? "", pickerOpen);
  const activeIndex = Math.min(activeOption, Math.max(0, options.length - 1));
  const listId = useId();
  const latest = useLatest({ mention, pickerOpen, options, activeIndex });

  const pick = useStableHandler((index: number) => {
    const { options: offered, mention: token } = latest.current;
    const option = offered[index];
    const field = fieldRef.current;
    if (!option || !token || !field) return;
    const applied = applyMention(field.value, token, option.inserted);
    apply(applied.text, applied.caret);
    setMention(undefined);
  });

  const sync = () => {
    const field = fieldRef.current;
    if (!field) return;
    const caret = field.selectionStart;
    const next = document.activeElement === field && caret === field.selectionEnd ? mentionAt(field.value, caret) : undefined;
    const current = latest.current.mention;
    if (queryOf(next) !== queryOf(current)) setActiveOption(0);
    if (!sameMention(next, current)) setMention(next);
    // A picker Escape closed stays closed only while its mention is being typed.
    if (startOf(next) !== startOf(current)) setDismissed(null);
  };

  const onKey = useStableHandler((event: KeyboardEvent): boolean => {
    const { pickerOpen: open, options: offered, activeIndex: index, mention: token } = latest.current;
    if (!open || !token || isImeKey(event) || event.altKey || event.ctrlKey || event.metaKey) return false;
    const count = offered.length;
    if ((event.key === "ArrowDown" || event.key === "ArrowUp") && count > 0) {
      setActiveOption((index + (event.key === "ArrowDown" ? 1 : count - 1)) % count);
      return true;
    }
    if ((event.key === "Enter" || event.key === "Tab") && !event.shiftKey && count > 0) {
      pick(index);
      return true;
    }
    if (event.key === "Escape") {
      event.stopPropagation();
      setDismissed(token.start);
      return true;
    }
    return false;
  });

  const close = useStableHandler(() => setMention(undefined));

  return { pickerOpen, listId, options, files, activeIndex, setActive: setActiveOption, sync: useStableHandler(sync), onKey, pick, close };
}
