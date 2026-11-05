// The gallery's scenarios for the keyboard shortcuts: Settings → Keyboard shortcuts as a browser and as
// the desktop app list it, a row recording a new combination, the reasons a combination is refused and what
// a changed shortcut looks like. Which commands a client lists, and how a key is written, follow the platform
// the gallery is viewed in (⌘ on an Apple keyboard, Ctrl elsewhere), or the scenario's `host` where it gives one: the
// steps press combinations through `ui.chord`, which turns a portable binding into that host's keys.
import type { Binding } from "../../shortcuts/binding";
import type { Overrides } from "../../shortcuts/store";
import type { Ui, Step } from "../../fixtures/interact";
import type { Preferences } from "../../fixtures/preferences";
import type { Scenario } from "../scenario";
import { SAMPLE, snapshotState, WINDOWS_MACHINE } from "../../fixtures/builders";
import { MAC_HOST, WINDOWS_HOST } from "../../test/hosts";

const GROUP = "Settings";

const { console: console_, web, api, sessions } = SAMPLE;

const state = snapshotState({ consoles: [console_], projects: [web, api], sessions });

const openSection: Step[] = [
  (ui) => ui.press(ui.t("rail.settings")),
  (ui) => ui.press(ui.t("settings.section.shortcuts")),
];

const changeLabel = (ui: Ui, command: "newSession" | "quickCreate") =>
  ui.t("settings.shortcuts.changeLabel", { command: ui.t(`settings.shortcuts.command.${command}`) });

/** Enters recording on the row of `command`. */
const recordFor = (command: "newSession" | "quickCreate"): Step => (ui) => ui.press(changeLabel(ui, command));

const chord = (binding: Binding): Step => (ui) => ui.chord(binding);

/** The changes a user has made: valid on every platform, none equal to a default. */
const customised: Overrides = {
  focusMode: { code: "KeyG", primary: true, shift: true },
  quickCreate: { code: "KeyK", primary: true, shift: true },
  newSession: { code: "KeyO", primary: true },
};

/** A focus-mode key moved away and its default taken by another command, so putting it back is refused. */
const defaultTaken: Overrides = {
  focusMode: { code: "KeyG", primary: true, shift: true },
  quickCreate: { code: "KeyF", primary: true, shift: true },
};

/** Changes that use every modifier a Windows keyboard can bind (Ctrl, Alt, Shift), so the key caps of each show. */
const customisedWindows: Overrides = {
  focusMode: { code: "KeyG", primary: true, alt: true, shift: true },
  newSession: { code: "KeyO", primary: true, alt: true },
};

const customisedPreferences = (keyboardShortcuts: Overrides): Preferences => ({ keyboardShortcuts });

export const shortcutsScenarios: Scenario[] = [
  {
    id: "settings-shortcuts",
    group: GROUP,
    category: "normal",
    title: "Keyboard shortcuts in a browser",
    description:
      "Only what a browser has is listed: under Navigation the two Fixed region keys, then Focus mode with a Change button per command, then Application with Open settings alone, also with a Change button, each key drawn as key caps. Back and Forward, the console-session switch and the native menu's keys (Window and the editing keys of the system) are left out. Editing is listed with only Find in file, Fixed. A line above says shortcut changes apply only to the current browser, and there is no Restore defaults while every key is a default.",
    state,
    steps: openSection,
  },
  {
    id: "settings-shortcuts-desktop",
    group: GROUP,
    category: "normal",
    title: "Keyboard shortcuts in the desktop app",
    description:
      "The introduction says shortcut changes apply only to the current desktop app. Every group is listed: Navigation (Back and Forward appear only where the window itself reports macOS, then the console-session switch as ⌃⇥ and ⌃⇧⇥ (Tab drawn as the ⇥ symbol), each with a Change button), Focus mode, Application with Open settings, then Editing and Window below it, where the dialog scrolls and every row reads Fixed instead of Change.",
    desktopApp: true,
    host: MAC_HOST,
    state,
    steps: openSection,
  },
  {
    id: "settings-shortcuts-windows",
    group: GROUP,
    category: "normal",
    title: "Keyboard shortcuts in the Windows app",
    description:
      "The Windows app has no native menu bar and its window controls are the system's, so it lists what a browser does plus the console-session switch, under the desktop app's introduction: under Navigation, Next console session as Ctrl and Tab and Previous console session as Ctrl, Shift and Tab sit before the two Fixed region keys, each with a Change button (never Win, and no Back or Forward); the menu's editing and window keys are left out, and Open settings is listed and changeable, as the page itself handles it. The key caps are written as text, in Windows' order: Ctrl, Alt and Shift, never ⌘⌥⇧. Toggle focus mode and New session show their changed keys (Ctrl, Alt, Shift and G; Ctrl, Alt and O) with a Reset button; the rest, the console-session switch among them, keep their defaults (Ctrl+Shift+F, Ctrl+N, Ctrl+Shift+N, Ctrl+,) with no Reset. A menu's key caps are the same (see Menu of a project session on Windows).",
    desktopApp: true,
    host: WINDOWS_HOST,
    state: snapshotState({ consoles: [console_], projects: [web, api], sessions, ...WINDOWS_MACHINE }),
    preferences: customisedPreferences(customisedWindows),
    steps: openSection,
  },
  {
    id: "settings-shortcuts-customised",
    group: GROUP,
    category: "normal",
    title: "Keyboard shortcuts, some changed",
    description:
      "Toggle focus mode, New session with defaults and New session show their own keys, each with a Reset button after Change that the unchanged rows lack, and Restore defaults sits at the end of the intro line. Choose 560px to see the narrow layout: at 768px and narrower each row stacks its keys and buttons under its name, aligned to the end edge, and the intro line stacks Restore defaults under itself, also at the end edge.",
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
      "Reset was pressed on Toggle focus mode, whose default New session with defaults now has: the row is not recording, keeps its changed key with Change and Reset beside it, and a danger-coloured line under its name says the combination is already used for that command.",
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
      "Recording continues on the New session with defaults row after pressing the left arrow with the shortcut modifier; its line says that with it the arrow keys, Home, End, Page Up, Page Down, Backspace, Delete and Enter move the caret or edit text, so they cannot be used. This is the longest refusal and wraps over several lines: choose 560px to see it wrap under the row's name, where each row stacks at 768px and narrower.",
    state,
    steps: [...openSection, recordFor("quickCreate"), chord({ code: "ArrowLeft", primary: true })],
  },
];
