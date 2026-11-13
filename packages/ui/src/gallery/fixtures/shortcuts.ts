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
      "A changeable shortcut shows only its key caps, which are themselves the edit button: they darken on hover, and pressing them records a new combination. Fixed shortcuts have a lock in the reserved trailing slot. Navigation lists only the two region keys, Focus mode lists its three commands, Application lists Open settings and Editing lists Find. The introduction names the current browser as the scope, and Restore defaults is absent while every key is a default.",
    state,
    steps: openSection,
  },
  {
    id: "settings-shortcuts-desktop",
    group: GROUP,
    category: "normal",
    title: "Keyboard shortcuts in the desktop app",
    description:
      "The introduction names the current desktop app as the scope. Navigation includes Back and Forward and the console-session switch, with the key caps of each changeable combination themselves the edit button. Fixed region keys align with those buttons and carry a lock in the trailing slot. Focus mode and Application follow; scroll to Editing and Window to see their fixed keys with the same lock treatment.",
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
      "The Windows app lists the console-session switch before the fixed region keys, with no Back, Forward or native-menu commands. The key caps of each changeable combination are the edit button, written with Windows key names in Ctrl, Alt and Shift order rather than Apple glyphs. Toggle focus mode and New session show changed combinations with a reset icon in the reserved trailing slot; unchanged rows leave that slot empty and fixed rows show a lock. Open settings remains changeable, and the introduction names the current desktop app as the scope.",
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
      "Toggle focus mode, New session with defaults and New session show their changed key caps, with a reset icon in the reserved trailing slot. Unchanged rows keep the same spacing without a reset icon, and fixed rows use that slot for a lock. Restore defaults sits at the end of the introduction. Choose 560px to see each row stack its controls below its name while preserving their end alignment.",
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
      "The New session key caps give way to the prompt \"Press the new keys…\" with an X icon for canceling; the other rows keep their key caps and alignment. Press the prompt again or Escape to cancel without changing the shortcut.",
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
      "Resetting Toggle focus mode is refused because New session with defaults now holds its default. The row keeps its changed key caps and reset icon, with a danger-coloured explanation below its name; it is not recording.",
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
