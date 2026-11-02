// The gallery's scenarios for the keyboard shortcuts: Settings → Keyboard shortcuts as a browser and as
// the desktop app list it, a row recording a new combination, the reasons a combination is refused and what
// a changed shortcut looks like. Which commands a client lists, and how a key is written, follow the platform
// the gallery is viewed in (⌘ on an Apple keyboard, Ctrl elsewhere): the steps press combinations through
// `ui.chord`, which turns a portable binding into that platform's keys.
import type { Binding } from "../../shortcuts/binding";
import type { Overrides } from "../../shortcuts/store";
import type { Ui, Step } from "../../fixtures/interact";
import type { Preferences } from "../../fixtures/preferences";
import type { Scenario } from "../scenario";
import { SAMPLE, snapshotState } from "../../fixtures/builders";

const GROUP = "Settings";

const { console: console_, web, api, sessions } = SAMPLE;

const state = snapshotState({ consoles: [console_], projects: [web, api], sessions });

const openSection: Step[] = [
  (ui) => ui.press(ui.t("rail.settings")),
  (ui) => ui.press(ui.t("settings.section.shortcuts")),
];

const changeLabel = (ui: Ui, command: "newSession" | "lock") =>
  ui.t("settings.shortcuts.changeLabel", { command: ui.t(`settings.shortcuts.command.${command}`) });

/** Enters recording on the row of `command`. */
const recordFor = (command: "newSession" | "lock"): Step => (ui) => ui.press(changeLabel(ui, command));

const chord = (binding: Binding): Step => (ui) => ui.chord(binding);

/** The changes a user has made: valid on every platform, none equal to a default. */
const customised: Overrides = {
  focusMode: { code: "KeyG", primary: true, shift: true },
  lock: { code: "KeyK", primary: true, shift: true },
  newSession: { code: "KeyO", primary: true },
};

/** A focus-mode key moved away and its default taken by another command, so putting it back is refused. */
const defaultTaken: Overrides = {
  focusMode: { code: "KeyG", primary: true, shift: true },
  lock: { code: "KeyF", primary: true, shift: true },
};

const customisedPreferences = (keyboardShortcuts: Overrides): Preferences => ({ keyboardShortcuts });

export const shortcutsScenarios: Scenario[] = [
  {
    id: "settings-shortcuts",
    group: GROUP,
    category: "normal",
    title: "Keyboard shortcuts in a browser",
    description:
      "Only what a browser has is listed: under Navigation the two Fixed region keys, then Focus mode and Sessions with a Change button per command, each key drawn as key caps. Back and Forward, the console-session switch and the native-menu groups (Application, Window) are left out. Editing is listed with only Find in file, also Fixed. A line above says the changes apply to this device or browser only, and there is no Restore defaults while every key is a default.",
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
      "Toggle focus mode, Lock or unlock the session and New session show their own keys, each with a Reset button after Change that the unchanged rows lack, and Restore defaults sits at the end of the intro line. Choose 560px to see the narrow layout: below 640px each row stacks its keys and buttons under its name, aligned to the end edge, and the intro line stacks Restore defaults under itself, also at the end edge.",
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
    id: "settings-shortcuts-conflict",
    group: GROUP,
    category: "exceptional",
    title: "Keyboard shortcuts, a key another command has",
    description:
      "Recording continues on the New session row, which names the command that already has the combination, \"Toggle focus mode\", in a danger-coloured line under its name; nothing is saved. A combination the desktop app has, one without a modifier, one the window or the system keeps and a terminal control key refuse in the same line, each with its own wording.",
    state,
    steps: [...openSection, recordFor("newSession"), chord({ code: "KeyF", primary: true, shift: true })],
  },
  {
    id: "settings-shortcuts-reset-refused",
    group: GROUP,
    category: "exceptional",
    title: "Keyboard shortcuts, a default another command holds",
    description:
      "Reset was pressed on Toggle focus mode, whose default Lock or unlock the session now has: the row is not recording, keeps its changed key with Change and Reset beside it, and a danger-coloured line under its name says the combination is already used for that command.",
    state,
    preferences: customisedPreferences(defaultTaken),
    steps: [...openSection, (ui) => ui.press(ui.t("settings.shortcuts.resetLabel", { command: ui.t("settings.shortcuts.command.focusMode") }))],
  },
  {
    id: "settings-shortcuts-editing-key",
    group: GROUP,
    category: "exceptional",
    title: "Keyboard shortcuts, a key that edits text",
    description:
      "Recording continues on the Lock or unlock row after pressing the left arrow with the shortcut modifier; its line says that with it the arrow keys, Home, End, Page Up, Page Down, Backspace, Delete and Enter move the caret or edit text, so they cannot be used. This is the longest refusal and wraps over several lines: choose 560px to see it wrap under the row's name, where each row stacks below 640px.",
    state,
    steps: [...openSection, recordFor("lock"), chord({ code: "ArrowLeft", primary: true })],
  },
];
