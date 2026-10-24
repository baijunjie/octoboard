// The gallery's scenarios for the keyboard shortcuts: Settings → Keyboard shortcuts as a browser and as
// the desktop app list it, a row recording a new combination, the reasons a combination is refused, what
// a changed shortcut looks like, and a menu that shows one. Which commands a client lists, and how a key
// is written, follow the platform the gallery is viewed in (⌘ on an Apple keyboard, Ctrl elsewhere): the
// steps press combinations through `ui.chord`, which turns a portable binding into that platform's keys.
import type { Binding } from "../../shortcuts/binding";
import type { Overrides } from "../../shortcuts/store";
import { IS_MAC } from "../../useWindowShortcut";
import type { Ui } from "../interact";
import type { Preferences, Scenario, Step } from "../scenario";
import { SAMPLE, snapshotState } from "./builders";

const GROUP = "Settings";

const { console: console_, web, api, consoleSession, sessions } = SAMPLE;

const state = snapshotState({ consoles: [console_], projects: [web, api], sessions });

const openSection: Step[] = [
  (ui) => ui.press(ui.t("rail.settings")),
  (ui) => ui.press(ui.t("settings.section.shortcuts")),
];

const changeLabel = (ui: Ui, command: "newSession" | "lock" | "nextConsoleSession") =>
  ui.t("settings.shortcuts.changeLabel", { command: ui.t(`settings.shortcuts.command.${command}`) });

/** Enters recording on the row of `command`. */
const recordFor = (command: "newSession" | "lock" | "nextConsoleSession"): Step => (ui) => ui.press(changeLabel(ui, command));

const chord = (binding: Binding): Step => (ui) => ui.chord(binding);

/** The changes a user has made: valid on every platform, none equal to a default. */
const customised: Overrides = {
  focusMode: { code: "KeyG", primary: true, shift: true },
  lock: { code: "KeyK", primary: true, shift: true },
  newSession: { code: "KeyO", primary: true },
};

/** Changes among the first rows of the desktop app's list, which a narrow window shows without scrolling. */
const customisedAtTop: Overrides = {
  previousConsoleSession: { code: "KeyJ", primary: true },
};

const menuCustomised: Overrides = {
  focusMode: { code: "KeyG", primary: true, shift: true },
  lock: { code: "KeyK", primary: true, shift: true },
  pin: { code: "KeyU", primary: true, shift: true },
  copyId: { code: "KeyI", primary: true, shift: true },
};

/** A focus-mode key moved away and its default taken by another command, so putting it back is refused. */
const defaultTaken: Overrides = {
  focusMode: { code: "KeyG", primary: true, shift: true },
  lock: { code: "KeyF", primary: true, shift: true },
};

const customisedPreferences = (keyboardShortcuts: Overrides): Preferences => ({ keyboardShortcuts });

/** Control with the key, however the platform names it: ⌃C on an Apple keyboard, Ctrl+C elsewhere. */
const controlC: Binding = IS_MAC ? { code: "KeyC", other: true } : { code: "KeyC", primary: true };

export const shortcutsScenarios: Scenario[] = [
  {
    id: "settings-shortcuts",
    group: GROUP,
    category: "normal",
    title: "Keyboard shortcuts in a browser",
    description:
      "Only what a browser has is listed: under Navigation the two Fixed region keys, then Focus mode and Sessions with a Change button per command, each key drawn as key caps. Back and Forward, the console-session switch and the native-menu groups (Application, Editing, Window) are left out. A line above says the changes apply to this device or browser only, and there is no Restore defaults while every key is a default.",
    state,
    steps: openSection,
  },
  {
    id: "settings-shortcuts-desktop",
    group: GROUP,
    category: "normal",
    title: "Keyboard shortcuts in the desktop app",
    description:
      "Every group is listed: Navigation (Back and Forward appear only where the window itself reports macOS, the console-session switch as ⌃Tab and ⇧⌃Tab), Focus mode, Sessions, Application with Open settings, then Editing and Window below it, where the dialog scrolls and every row reads Fixed instead of Change.",
    desktopApp: true,
    state,
    steps: openSection,
  },
  {
    id: "settings-shortcuts-customised",
    group: GROUP,
    category: "normal",
    title: "Keyboard shortcuts, some changed",
    description:
      "Toggle focus mode, Lock or unlock the session and New session show their own keys, each with a Reset button after Change that the unchanged rows lack, and Restore defaults sits at the end of the intro line.",
    state,
    preferences: customisedPreferences(customised),
    steps: openSection,
  },
  {
    id: "settings-shortcuts-recording",
    group: GROUP,
    category: "normal",
    title: "Keyboard shortcuts, recording a new key",
    description:
      "The New session row has \"Press the new keys…\" in place of its key caps, and its button reads Cancel; the other rows stay as they were. Escape or Cancel leaves the row as it was.",
    state,
    steps: [...openSection, recordFor("newSession")],
  },
  {
    id: "settings-shortcuts-narrow",
    group: GROUP,
    category: "normal",
    title: "Keyboard shortcuts in a narrow window",
    description:
      "Below 640px each row stacks its keys and buttons under its name, and the intro line stacks Restore defaults under itself. Previous console session shows a changed key with Reset, and the reason a key was refused (Next console session, recording a key without a modifier) wraps under its name. Fixed to a window that narrow, which is the layout it checks.",
    width: 560,
    desktopApp: true,
    state,
    preferences: customisedPreferences(customisedAtTop),
    steps: [...openSection, recordFor("nextConsoleSession"), chord({ code: "KeyJ" })],
  },
  {
    id: "settings-shortcuts-conflict",
    group: GROUP,
    category: "exceptional",
    title: "Keyboard shortcuts, a key another command has",
    description:
      "Recording continues on the New session row, which names the command that already has the combination, \"Toggle focus mode\", in a danger-coloured line under its name; nothing is saved.",
    state,
    steps: [...openSection, recordFor("newSession"), chord({ code: "KeyF", primary: true, shift: true })],
  },
  {
    id: "settings-shortcuts-conflict-desktop",
    group: GROUP,
    category: "exceptional",
    title: "Keyboard shortcuts, a key the desktop app has",
    description:
      "Recording continues on the New session row after pressing the combination of Minimize, a command this browser does not list; its line says the combination is already used for Minimize in the desktop app.",
    state,
    steps: [...openSection, recordFor("newSession"), chord({ code: "KeyM", primary: true })],
  },
  {
    id: "settings-shortcuts-reset-refused",
    group: GROUP,
    category: "exceptional",
    title: "Keyboard shortcuts, a default another command holds",
    description:
      "Reset was pressed on Toggle focus mode, whose default Lock or unlock the session now has: its row keeps its changed key and says the combination is already used for that command, and Reset stays beside it.",
    state,
    preferences: customisedPreferences(defaultTaken),
    steps: [...openSection, (ui) => ui.press(ui.t("settings.shortcuts.resetLabel", { command: ui.t("settings.shortcuts.command.focusMode") }))],
  },
  {
    id: "settings-shortcuts-needs-modifier",
    group: GROUP,
    category: "exceptional",
    title: "Keyboard shortcuts, a key without a modifier",
    description:
      "Recording continues on the Lock or unlock row, whose line says to hold ⌘ (Ctrl outside an Apple keyboard) with the key.",
    state,
    steps: [...openSection, recordFor("lock"), chord({ code: "KeyJ" })],
  },
  {
    id: "settings-shortcuts-reserved-key",
    group: GROUP,
    category: "exceptional",
    title: "Keyboard shortcuts, a key the window keeps",
    description: "Recording continues on the Lock or unlock row, whose line says Escape, F6 and the context-menu key keep their own meaning.",
    state,
    steps: [...openSection, recordFor("lock"), chord({ code: "F6", primary: true })],
  },
  {
    id: "settings-shortcuts-editing-key",
    group: GROUP,
    category: "exceptional",
    title: "Keyboard shortcuts, a key that edits text",
    description:
      "Recording continues on the Lock or unlock row after pressing the left arrow with the shortcut modifier; its line says that with it the arrow keys, Home, End, Page Up, Page Down, Backspace, Delete and Enter move the caret or edit text, so they cannot be used.",
    state,
    steps: [...openSection, recordFor("lock"), chord({ code: "ArrowLeft", primary: true })],
  },
  {
    id: "settings-shortcuts-os-reserved",
    group: GROUP,
    category: "exceptional",
    title: "Keyboard shortcuts, a combination the system keeps",
    description: "Recording continues on the Lock or unlock row, whose line says the system keeps the combination for itself.",
    state,
    steps: [...openSection, recordFor("lock"), chord({ code: "Tab", primary: true })],
  },
  {
    id: "settings-shortcuts-control-key",
    group: GROUP,
    category: "exceptional",
    title: "Keyboard shortcuts, a terminal control key",
    description:
      "Recording continues on the Lock or unlock row after pressing Control and C. On an Apple keyboard its line says Control with a key is a terminal control key and to add ⌘; elsewhere it says Ctrl+C interrupts the program in the terminal, so it cannot be used.",
    state,
    steps: [...openSection, recordFor("lock"), chord(controlC)],
  },
  {
    id: "sidebar-session-menu-customised-shortcuts",
    group: "Sidebar sessions",
    category: "normal",
    title: "Session menu with changed shortcuts",
    description:
      "The Hub's menu shows the keys the user has set, not the defaults: Lock, Pin, Focus mode and Copy session ID carry their changed keys, and Archive its default.",
    state,
    preferences: customisedPreferences(menuCustomised),
    steps: [(ui) => ui.press(ui.t("sidebar.session.actions", { title: consoleSession.title }))],
  },
];
