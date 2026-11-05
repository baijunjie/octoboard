import { isKnownCode, onHost, sameKeys, type Binding, type ModifierHost } from "./binding";
import { ALL_COMMANDS, REBINDABLE_COMMANDS, type CommandId } from "./commands";

/** Why a combination cannot be a command's binding. `conflict` also says which command holds it. */
export type ShortcutProblem =
  | { reason: "unsupportedKey" | "shiftedKey" | "needsModifier" | "reservedKey" | "editingKey" | "osReserved" | "terminalControl" | "interrupt" }
  | { reason: "conflict"; with: string };

/** Keys that widgets and the window use as they are, whatever is held with them. */
const RESERVED_CODES = new Set(["Escape", "F6", "ContextMenu"]);

/** The keys that move the caret or edit text, which a shortcut with ⌘ or Ctrl would take from every text field and the terminal. */
const EDITING_CODES = new Set(["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End", "PageUp", "PageDown", "Backspace", "Delete", "Enter"]);

/** The number and symbol keys, which with Shift held type another character. */
const SHIFTED_CODE = /^(Digit\d|Minus|Equal|BracketLeft|BracketRight|Semicolon|Quote|Backquote|Backslash|Comma|Period|Slash)$/;

/** The combinations of the operating system on an Apple keyboard that never reach the page. */
const OS_RESERVED_CODES = new Set(["Space", "Backquote"]);

/**
 * Whether `stored` can be anyone's binding on `host`, whatever the others are. It is judged as the keys it is there
 * (`onHost`), so a binding carried from another platform is refused where those keys cannot be a shortcut.
 *
 * Tab takes Control, the browsers' tab key on every system and the console-session switch's: ⌃Tab with any of ⌥ and
 * ⇧ on an Apple keyboard, whose ⌘Tab is the system's application switcher; Ctrl+Tab and Ctrl+Shift+Tab elsewhere,
 * where the system keeps every Tab with the Windows key (Task View) or with Alt (Ctrl+Alt+Tab is its switcher).
 */
function problemOfKey(stored: Binding, host: ModifierHost): ShortcutProblem | undefined {
  const apple = host.modifier === "meta";
  const binding = onHost(stored, host);
  if (!isKnownCode(binding.code)) return { reason: "unsupportedKey" };
  if (RESERVED_CODES.has(binding.code)) return { reason: "reservedKey" };
  if (!binding.primary) {
    if (apple && binding.other && binding.code === "Tab") return undefined;
    if (!binding.other) return { reason: "needsModifier" };
    return { reason: apple ? "terminalControl" : "osReserved" };
  }
  if (EDITING_CODES.has(binding.code)) return { reason: "editingKey" };
  if (apple && OS_RESERVED_CODES.has(binding.code) && !binding.other) return { reason: "osReserved" };
  if (binding.code === "Tab") return !apple && !binding.other && !binding.alt ? undefined : { reason: "osReserved" };
  // Ctrl+C interrupts the terminal's process; ⌘C is the Copy of the native menu, a conflict of its own.
  if (!apple && binding.code === "KeyC" && !binding.shift && !binding.alt && !binding.other) return { reason: "interrupt" };
  return undefined;
}

/**
 * Why `binding` cannot be the binding of the command `id`, or `undefined` when it can. `current` is
 * every rebindable command's binding as it stands. A combination another command holds is refused
 * whether or not that command exists on this client, as is one of the fixed keys: the same keys on
 * `host` (`sameKeys`), however each is written.
 */
export function validateBinding(id: CommandId, binding: Binding, current: Readonly<Record<CommandId, Binding>>, host: ModifierHost): ShortcutProblem | undefined {
  const keyProblem = problemOfKey(binding, host);
  if (keyProblem) return keyProblem;
  // The command the macOS native menu carries: AppKit matches a menu's key against the character the
  // key types with Shift applied, so ⇧ with a number or symbol key never fires there. Refused on every
  // platform, since a binding travels between machines in a configuration document.
  if (id === "openSettings" && binding.shift && SHIFTED_CODE.test(binding.code)) return { reason: "shiftedKey" };
  const fixed = ALL_COMMANDS.find((command) => !command.rebindable && sameKeys(command.binding, binding, host));
  if (fixed) return { reason: "conflict", with: fixed.id };
  const other = REBINDABLE_COMMANDS.find((command) => command.id !== id && sameKeys(current[command.id], binding, host));
  if (other) return { reason: "conflict", with: other.id };
  return undefined;
}
