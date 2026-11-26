import { useSyncExternalStore } from "react";

import type { Daemon } from "../store";
import { quotedText } from "../terminal/quotedText";

/** A file attached to the message being written. `path` is where the daemon holds it (or, for a file dropped in the
 * desktop application's own window, its own path on this computer), absent while it is still being sent; `image` puts
 * it in the message's `images`, for an agent that takes images, and anything else in its `files`. */
export interface ComposerAttachment {
  id: string;
  name: string;
  image: boolean;
  path?: string;
}

/** What the composer holds for one session: the text and attachments being written, and, while a queued message is
 * being edited in it, that message's id and what it held when the edit began (to send only what changed), with the
 * new message that was being written before, which comes back once the edit ends, and `restoreMode`, the mode the
 * message waited in before the edit held it back as manual (`when_idle`, so the daemon does not send it mid-edit), put
 * back when the edit ends. */
export interface ComposerDraft {
  text: string;
  attachments: readonly ComposerAttachment[];
  editing?: {
    message: string;
    images: readonly string[];
    files: readonly string[];
    set: { text: string; attachments: readonly ComposerAttachment[] };
    restoreMode?: "when_idle";
  };
}

export const EMPTY_DRAFT: ComposerDraft = { text: "", attachments: [] };

/** A composer on screen for a session, which focus can be put in. */
export interface ComposerEditor {
  focus: () => void;
}

interface DraftStore {
  drafts: Map<string, ComposerDraft>;
  listeners: Set<() => void>;
  editors: Map<string, ComposerEditor>;
}

// Held per daemon store, so that another daemon (or a test's own) starts with none, and outside any component, so that
// the text survives switching sessions and the composer being turned off and on. Only what this window's user typed:
// it is not the queue, no other window sees it, and a reload loses it.
const held = new WeakMap<Daemon["store"], DraftStore>();

function storeOf(store: Daemon["store"]): DraftStore {
  let drafts = held.get(store);
  if (drafts === undefined) {
    drafts = { drafts: new Map(), listeners: new Set(), editors: new Map() };
    held.set(store, drafts);
  }
  return drafts;
}

/** The draft held for `session`, the empty one if nothing is. */
export function composerDraft(store: Daemon["store"], session: string): ComposerDraft {
  return held.get(store)?.drafts.get(session) ?? EMPTY_DRAFT;
}

/** Holds `draft` as `session`'s, dropping the drafts of sessions the store no longer has, and tells every composer. */
export function setComposerDraft(store: Daemon["store"], session: string, draft: ComposerDraft): void {
  const drafts = storeOf(store);
  if (drafts.drafts.get(session) === draft) return;
  if (draft.text === "" && draft.attachments.length === 0 && draft.editing === undefined) drafts.drafts.delete(session);
  else drafts.drafts.set(session, draft);
  const known = store.getState().sessions;
  for (const id of drafts.drafts.keys()) {
    if (!known.has(id)) drafts.drafts.delete(id);
  }
  for (const listener of drafts.listeners) listener();
}

/** Changes `session`'s draft by `change`, given the draft as it stands. */
export function updateComposerDraft(store: Daemon["store"], session: string, change: (draft: ComposerDraft) => ComposerDraft): void {
  setComposerDraft(store, session, change(composerDraft(store, session)));
}

/** `session`'s draft, re-rendering the caller whenever it changes. */
export function useComposerDraft(store: Daemon["store"], session: string | undefined): ComposerDraft {
  return useSyncExternalStore(
    (listener) => {
      const drafts = storeOf(store);
      drafts.listeners.add(listener);
      return () => drafts.listeners.delete(listener);
    },
    () => (session === undefined ? EMPTY_DRAFT : composerDraft(store, session)),
  );
}

/** Registers the composer on screen for `session`; returns the function that lets it go. */
export function registerComposerEditor(store: Daemon["store"], session: string, editor: ComposerEditor): () => void {
  const { editors } = storeOf(store);
  editors.set(session, editor);
  return () => {
    if (editors.get(session) === editor) editors.delete(session);
  };
}

/** `text` added to the end of `current`, on a line of its own where `current` has text that does not end a line. A
 * `current` of whitespace only counts as empty, since there is nothing to send in it: `text` replaces it. */
function appended(current: string, text: string): string {
  if (current.trim() === "") return text;
  if (current.endsWith("\n")) return current + text;
  return `${current}\n${text}`;
}

/** Puts `text` at the end of `session`'s composer, on a line of its own, without sending it: the entry point for what
 * writes into the composer from elsewhere (Quote, Add as context). It works whether or not the composer is mounted, and
 * the text is there when it next shows. Whether the composer is on at all is the caller's question
 * (`sessionComposerEnabled`), since with it off such text goes into the terminal's input instead. */
export function appendToComposer(store: Daemon["store"], session: string, text: string): void {
  updateComposerDraft(store, session, (draft) => ({ ...draft, text: appended(draft.text, text) }));
}

/** Puts `text` at the end of `session`'s composer as a block quote (`quotedText`), on a line of its own, without sending
 * it: what Quote writes while the composer takes the input. Text that quotes to nothing writes nothing. */
export function quoteIntoComposer(store: Daemon["store"], session: string, text: string): void {
  const quoted = quotedText(text);
  if (quoted !== "") appendToComposer(store, session, quoted);
}

/** Puts keyboard focus in `session`'s composer where it is on screen; returns whether it was. */
export function focusComposer(store: Daemon["store"], session: string): boolean {
  const editor = held.get(store)?.editors.get(session);
  editor?.focus();
  return editor !== undefined;
}
