import { IS_MAC } from "../useWindowShortcut";
import { bindingsEqual, isKnownCode, type Binding } from "./binding";
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

/** Whether `binding` can be anyone's binding, whatever the others are. */
function problemOfKey(binding: Binding): ShortcutProblem | undefined {
  if (!isKnownCode(binding.code)) return { reason: "unsupportedKey" };
  if (RESERVED_CODES.has(binding.code)) return { reason: "reservedKey" };
  if (!binding.primary) {
    // ⌃Tab is the one combination without ⌘ or Ctrl that is allowed: it is the console-session switch's own.
    if (binding.other && binding.code === "Tab") return undefined;
    if (!binding.other) return { reason: "needsModifier" };
    return { reason: IS_MAC ? "terminalControl" : "osReserved" };
  }
  if (EDITING_CODES.has(binding.code)) return { reason: "editingKey" };
  if (IS_MAC && OS_RESERVED_CODES.has(binding.code) && !binding.other) return { reason: "osReserved" };
  if (binding.code === "Tab") return { reason: "osReserved" };
  // Ctrl+C interrupts the terminal's process; ⌘C is the Copy of the native menu, a conflict of its own.
  if (!IS_MAC && binding.code === "KeyC" && !binding.shift && !binding.alt && !binding.other) return { reason: "interrupt" };
  return undefined;
}

/**
 * Why `binding` cannot be the binding of the command `id`, or `undefined` when it can. `current` is
 * every rebindable command's binding as it stands. A combination another command holds is refused
 * whether or not that command exists on this client, as is one of the fixed keys.
 */
export function validateBinding(id: CommandId, binding: Binding, current: Readonly<Record<CommandId, Binding>>): ShortcutProblem | undefined {
  const keyProblem = problemOfKey(binding);
  if (keyProblem) return keyProblem;
  // The command only the native menu reaches: AppKit matches a menu's key against the character the
  // key types with Shift applied, so ⇧ with a number or symbol key never fires. Refused on every
  // platform, since a binding travels between machines in a configuration document.
  if (id === "openSettings" && binding.shift && SHIFTED_CODE.test(binding.code)) return { reason: "shiftedKey" };
  const fixed = ALL_COMMANDS.find((command) => !command.rebindable && bindingsEqual(command.binding, binding));
  if (fixed) return { reason: "conflict", with: fixed.id };
  const other = REBINDABLE_COMMANDS.find((command) => command.id !== id && bindingsEqual(current[command.id], binding));
  if (other) return { reason: "conflict", with: other.id };
  return undefined;
}
