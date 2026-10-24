import type { PlainMessageKey } from "../i18n/catalog";
import { IS_MAC } from "../useWindowShortcut";
import type { Binding } from "./binding";

/** Where the shortcuts are listed under, in the order of the list. */
export const SHORTCUT_GROUPS = ["navigation", "focus", "sessions", "application", "editing", "window"] as const;
export type ShortcutGroup = (typeof SHORTCUT_GROUPS)[number];

/** Which clients a shortcut exists on: `app` is the desktop app (`windowChrome`), `macApp` that on
 * macOS, and `nativeMenu` where the shell has a native menu bar. */
export type ShortcutAvailability = "always" | "app" | "macApp" | "nativeMenu";

/** What the current client has, which `isAvailable` tests a shortcut against. */
export interface ClientCapabilities {
  windowChrome: boolean;
  nativeMenu: boolean;
}

export function isAvailable(availability: ShortcutAvailability, client: ClientCapabilities): boolean {
  switch (availability) {
    case "always": return true;
    case "app": return client.windowChrome;
    case "macApp": return client.windowChrome && IS_MAC;
    case "nativeMenu": return client.nativeMenu;
  }
}

export interface ShortcutCommand {
  id: string;
  label: PlainMessageKey;
  group: ShortcutGroup;
  binding: Binding;
  availability: ShortcutAvailability;
  /** The user can bind it to another key. The others are listed so every key is in one place. */
  rebindable: boolean;
}

const primary = (code: string, modifiers: Omit<Binding, "code" | "primary"> = {}): Binding => ({ code, primary: true, ...modifiers });

/** The commands the user can rebind, in the order they are listed in, with their defaults. */
const REBINDABLE = [
  { id: "back", label: "settings.shortcuts.command.back", group: "navigation", binding: primary("BracketLeft"), availability: "macApp" },
  { id: "forward", label: "settings.shortcuts.command.forward", group: "navigation", binding: primary("BracketRight"), availability: "macApp" },
  { id: "nextConsoleSession", label: "settings.shortcuts.command.nextConsoleSession", group: "navigation", binding: { code: "Tab", other: true }, availability: "app" },
  { id: "previousConsoleSession", label: "settings.shortcuts.command.previousConsoleSession", group: "navigation", binding: { code: "Tab", other: true, shift: true }, availability: "app" },
  { id: "focusMode", label: "settings.shortcuts.command.focusMode", group: "focus", binding: primary("KeyF", { shift: true }), availability: "always" },
  { id: "newSession", label: "settings.shortcuts.command.newSession", group: "focus", binding: primary("KeyN"), availability: "always" },
  { id: "quickCreate", label: "settings.shortcuts.command.quickCreate", group: "focus", binding: primary("KeyN", { shift: true }), availability: "always" },
  { id: "lock", label: "settings.shortcuts.command.lock", group: "sessions", binding: primary("KeyL", { shift: true }), availability: "always" },
  { id: "pin", label: "settings.shortcuts.command.pin", group: "sessions", binding: primary("KeyP", { shift: true }), availability: "always" },
  { id: "archive", label: "settings.shortcuts.command.archive", group: "sessions", binding: primary("KeyA", { shift: true }), availability: "always" },
  { id: "copyId", label: "settings.shortcuts.command.copyId", group: "sessions", binding: primary("KeyC", { shift: true }), availability: "always" },
  { id: "openSettings", label: "settings.shortcuts.command.openSettings", group: "application", binding: primary("Comma"), availability: "nativeMenu" },
] as const satisfies readonly Omit<ShortcutCommand, "rebindable">[];

export type CommandId = (typeof REBINDABLE)[number]["id"];

/** The keys the window or the native menu own, listed but not changeable. */
const FIXED: readonly Omit<ShortcutCommand, "rebindable">[] = [
  { id: "nextRegion", label: "settings.shortcuts.command.nextRegion", group: "navigation", binding: { code: "F6" }, availability: "always" },
  { id: "previousRegion", label: "settings.shortcuts.command.previousRegion", group: "navigation", binding: { code: "F6", shift: true }, availability: "always" },
  { id: "undo", label: "settings.shortcuts.command.undo", group: "editing", binding: primary("KeyZ"), availability: "nativeMenu" },
  { id: "redo", label: "settings.shortcuts.command.redo", group: "editing", binding: primary("KeyZ", { shift: true }), availability: "nativeMenu" },
  { id: "cut", label: "settings.shortcuts.command.cut", group: "editing", binding: primary("KeyX"), availability: "nativeMenu" },
  { id: "copy", label: "settings.shortcuts.command.copy", group: "editing", binding: primary("KeyC"), availability: "nativeMenu" },
  { id: "paste", label: "settings.shortcuts.command.paste", group: "editing", binding: primary("KeyV"), availability: "nativeMenu" },
  { id: "selectAll", label: "settings.shortcuts.command.selectAll", group: "editing", binding: primary("KeyA"), availability: "nativeMenu" },
  { id: "hide", label: "settings.shortcuts.command.hide", group: "window", binding: primary("KeyH"), availability: "nativeMenu" },
  { id: "hideOthers", label: "settings.shortcuts.command.hideOthers", group: "window", binding: primary("KeyH", { alt: true }), availability: "nativeMenu" },
  { id: "fullScreen", label: "settings.shortcuts.command.fullScreen", group: "window", binding: primary("KeyF", { other: true }), availability: "nativeMenu" },
  { id: "minimize", label: "settings.shortcuts.command.minimize", group: "window", binding: primary("KeyM"), availability: "nativeMenu" },
  { id: "closeWindow", label: "settings.shortcuts.command.closeWindow", group: "window", binding: primary("KeyW"), availability: "nativeMenu" },
  { id: "quit", label: "settings.shortcuts.command.quit", group: "window", binding: primary("KeyQ"), availability: "nativeMenu" },
];

export const REBINDABLE_COMMANDS: readonly (ShortcutCommand & { id: CommandId })[] = REBINDABLE.map((command) => ({ ...command, rebindable: true }));

export const FIXED_COMMANDS: readonly ShortcutCommand[] = FIXED.map((command) => ({ ...command, rebindable: false }));

/** Every listed command, grouped in list order. */
export const ALL_COMMANDS: readonly ShortcutCommand[] = [...REBINDABLE_COMMANDS, ...FIXED_COMMANDS];

export function isCommandId(id: string): id is CommandId {
  return REBINDABLE_COMMANDS.some((command) => command.id === id);
}

export function defaultBinding(id: CommandId): Binding {
  return REBINDABLE_COMMANDS.find((command) => command.id === id)!.binding;
}
