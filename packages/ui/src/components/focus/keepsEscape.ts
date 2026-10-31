/** Elements that take typed text, as the Escape listener sees them: xterm.js's hidden `textarea`
 * is not one, though it is one to the DOM, for the terminal reads its keys itself. */
const EDITABLE = 'input, textarea:not(.xterm-helper-textarea), [contenteditable]:not([contenteditable="false"])';

/** The input types that take typed text. Any other `input` (a checkbox, a radio, a range, a
 * button) has a `value` that is no text the user typed, and no Escape of its own to keep. `number`
 * is left out: its `value` is a number the field may have been given, so it would hold text
 * whatever was typed, and nothing in it clears on Escape. */
const TEXT_INPUT_TYPES = new Set(["text", "search", "email", "url", "tel", "password"]);

/** Whether `target` is a field with text in it, whose Escape comes before the pane's: the field
 * clears itself, and the pane is left for the next Escape, with the field empty. A field already
 * empty has nothing to clear, so the key goes on to the pane; so does one that cannot be edited
 * (read-only, disabled), which has no Escape of its own. */
export function holdsText(target: Element | null): boolean {
  const field = target?.closest(EDITABLE);
  if (!field) return false;
  if (field instanceof HTMLInputElement) {
    return TEXT_INPUT_TYPES.has(field.type) && !field.readOnly && !field.disabled && field.value !== "";
  }
  if (field instanceof HTMLTextAreaElement) return !field.readOnly && !field.disabled && field.value !== "";
  // Anything else matched is `contenteditable`.
  return field.textContent !== "";
}

/** Whether Escape aimed at `target` is the target's own rather than the floating pane's: a text
 * field holding text (`holdsText`), or a shell terminal in the right pane, whose programs (vim,
 * less, fzf) need the key and which keeps Tab as well, so the pane is dismissed with its close
 * control, its toggle or by the pointer leaving instead. The agent terminal in the centre is not
 * one: Escape there still dismisses the pane. */
export function keepsEscape(target: Element | null): boolean {
  return holdsText(target) || target?.closest("[data-shell-terminal]") != null;
}
