import { usePlatform } from "../platform/react";
import { bindingOf, sameKeys, type ModifierHost } from "../shortcuts/binding";
import { ALL_COMMANDS, clientCapabilities, defaultBinding, isAvailable, type ClientPlatform as CapabilityPlatform, type CommandId } from "../shortcuts/commands";
import { bindingFor, matchesCommand } from "../shortcuts/store";
import { useWindowShortcut } from "../shortcuts/useWindowShortcut";
import { resetTerminalFontSize, stepTerminalFontSize } from "./terminalFontSize";

type FontSizeCommand = Extract<CommandId, "increaseTerminalFontSize" | "decreaseTerminalFontSize" | "resetTerminalFontSize">;

const COMMANDS: readonly FontSizeCommand[] = ["increaseTerminalFontSize", "decreaseTerminalFontSize", "resetTerminalFontSize"];

/** The character a command's default keys also type on a layout that has it elsewhere than on its default key: `+` is
 * a key of its own on many layouts, and `-` is not beside `0` on all of them. `+` is typed with Shift on a US keyboard,
 * so Shift is let through for it alone. */
const LAYOUT_CHARACTER: Partial<Record<FontSizeCommand, { key: string; shift: boolean }>> = {
  increaseTerminalFontSize: { key: "+", shift: true },
  decreaseTerminalFontSize: { key: "-", shift: false },
};

/** Whether the command's keys are its default ones, the keys its layout character stands in for. */
function onDefaultKeys(id: FontSizeCommand, host: ModifierHost): boolean {
  return sameKeys(bindingFor(id), defaultBinding(id), host);
}

/** The font size command whose layout character `event` types, for the command on its default keys and the key held
 * with the primary modifier alone (Shift too for `+`), unless the combination is the keys of a command that is not a
 * font size one and exists on this client, which keeps it. */
function matchTypedCharacter(event: KeyboardEvent, platform: ClientPlatform): FontSizeCommand | undefined {
  const { host } = platform;
  const pressed = bindingOf(event, host);
  if (!pressed.primary || pressed.alt || pressed.other) return undefined;
  const found = COMMANDS.find((id) => {
    const character = LAYOUT_CHARACTER[id];
    return character && event.key === character.key && (character.shift || !pressed.shift) && onDefaultKeys(id, host);
  });
  if (!found) return undefined;
  const client = clientCapabilities(platform);
  const taken = ALL_COMMANDS.some((command) => {
    if ((COMMANDS as readonly string[]).includes(command.id) || !isAvailable(command.availability, client)) return false;
    const binding = command.rebindable ? bindingFor(command.id as CommandId) : command.binding;
    return sameKeys(binding, pressed, host);
  });
  return taken ? undefined : found;
}

/** What `matchFontSizeShortcut` reads of the platform adapter. */
type ClientPlatform = CapabilityPlatform & { host: ModifierHost };

/** Whether `event` came from the keyboard focus being in a terminal. */
function inTerminal(event: KeyboardEvent): boolean {
  return event.target instanceof Element && event.target.closest(".xterm") !== null;
}

/** The font size command `event` is, only while a terminal has focus: elsewhere ⌘= and its kin are the browser's page
 * zoom, which a web client's user keeps. A command rebound by the user is matched by its binding alone, the layout
 * characters being the default keys' own. A character a layout types wins over another font size command's key by
 * code: the key that types `+` is `Minus` on a Nordic layout, and the one that types `-` is `Equal` on Turkish Q, each
 * the command of its character while that keeps its default keys and nothing otherwise, never the opposite one. */
export function matchFontSizeShortcut(event: KeyboardEvent, platform: ClientPlatform): FontSizeCommand | undefined {
  if (!inTerminal(event)) return undefined;
  const { host } = platform;
  const byCode = COMMANDS.find((id) => matchesCommand(id, event, host));
  if (byCode && !onDefaultKeys(byCode, host)) return byCode;
  const typed = matchTypedCharacter(event, platform);
  if (typed) return typed;
  const typesAnother = COMMANDS.some((id) => id !== byCode && LAYOUT_CHARACTER[id]?.key === event.key);
  return typesAnother ? undefined : byCode;
}

/** Changes the terminals' font size from the keyboard (⌘= or ⌘+, ⌘-, ⌘0; Ctrl elsewhere) while a terminal has
 * focus; mounted once for the window. */
export function useTerminalFontShortcuts(): void {
  const platform = usePlatform();
  useWindowShortcut(
    (event) => matchFontSizeShortcut(event, platform),
    (command) => {
      if (command === "increaseTerminalFontSize") stepTerminalFontSize(1);
      else if (command === "decreaseTerminalFontSize") stepTerminalFontSize(-1);
      else resetTerminalFontSize();
    },
  );
}
