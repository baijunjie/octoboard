import type { Preferences, Scenario } from "../scenario";
import type { Ui } from "../interact";
import { SAMPLE, shellTerminalOf, snapshotState } from "./builders";
import { terminalClosed, terminalOpened } from "./liveChanges";

const GROUP = "Terminal panel";

const { console: console_, web, api, sessions } = SAMPLE;

const stateOf = (shellTerminals: ReturnType<typeof shellTerminalOf>[]) =>
  snapshotState({ consoles: [console_], projects: [web, api], sessions, shellTerminals });

/** The Terminal panel alone in the stack (Files closed), so the panel takes the pane's height. */
const TERMINAL_ONLY: Preferences = { panelStacks: { project: { open: ["terminal"], shares: {} } } };

/** The Terminal panel alone, in a pane widened so that three tabs fit the strip unclipped: at the
 * default width they already scroll, which "Many tabs" is for. */
const WIDE_TERMINAL: Preferences = { ...TERMINAL_ONLY, asideWidth: 560 };

const openWebsite = (ui: Ui) => ui.press(ui.session("Fix the summary layout"));
/** Opens the session, then lets the Files panel read its folder, which draws nothing until it has. */
const openWebsiteWithFiles = [openWebsite, (ui: Ui) => ui.wait(400)];

/** The header's actions button for the shown tab, named after it. */
const headerMenu = (ui: Ui, title: string) => ui.press(ui.t("shellTerminal.actions", { title }));

const oneShell = [shellTerminalOf("sh-1", web, "zsh", 0)];

// The newest tab is the one shown, as it is for a project whose last chosen tab is gone.
const severalShells = [
  shellTerminalOf("sh-1", web, "zsh", 0),
  shellTerminalOf("sh-2", web, "pnpm dev", 1, { busy: true }),
  shellTerminalOf("sh-3", web, "git log", 2),
];

const MANY_TITLES = [
  "zsh", "pnpm dev", "git log", "cargo watch", "docker compose logs", "ssh staging", "htop", "~/code/website/src/summary",
  "vitest --watch", "tail -f server.log", "psql", "node",
];
const manyShells = MANY_TITLES.map((title, index) => shellTerminalOf(`sh-${index + 1}`, web, title, index));

export const terminalPanelScenarios: Scenario[] = [
  {
    id: "shell-panel-one-tab",
    group: GROUP,
    category: "normal",
    title: "Terminal panel with one tab",
    description:
      "The Terminal panel alone in the right pane, as the top bar's Terminal toggle (pressed) opens it. Its header runs the Terminal name, the tab strip with the single shell selected and its close mark, New terminal, the tab's actions menu, then Maximize panel, then Close panel: Terminal offers maximize even as the only panel. The shell draws on the card's own terminal background, inset as the agent terminal is.",
    state: stateOf(oneShell),
    preferences: TERMINAL_ONLY,
    steps: [openWebsite],
  },
  {
    id: "shell-panel-tabs",
    group: GROUP,
    category: "normal",
    title: "Several tabs",
    description:
      "Three shells in the order they were started, the newest selected with the raised fill and the others muted until hovered. The pane is widened to 560px so the three tabs fit the strip whole, as at the default width they already scroll (see Many tabs). Hover or select a tab to see its close mark; press New terminal for another tab, selected at once. The \"Live changes\" buttons make what another window would: its new terminal joins the strip at the end without being selected, and closing the shown one hands the view to a neighbour, and reopening it afterwards brings it back without selecting it, so the view stays on the neighbour. Tabs fade in or out while the ones after them slide into place; with reduced motion they appear and disappear at once.",
    state: stateOf(severalShells),
    preferences: WIDE_TERMINAL,
    steps: [openWebsite],
    liveChanges: [
      terminalOpened("open-htop", "Another window opens htop", shellTerminalOf("sh-4", web, "htop", 3)),
      terminalClosed("close-htop", "Another window closes htop", "sh-4"),
      terminalClosed("close-git-log", "Another window closes git log", "sh-3"),
      terminalOpened("open-git-log", "Another window reopens git log", severalShells[2]),
    ],
  },
  {
    id: "shell-panel-many-tabs",
    group: GROUP,
    category: "normal",
    title: "Many tabs, the strip scrolling",
    description:
      "Twelve shells, more than the strip holds: the strip scrolls, with the Terminal title kept whole and the scroll buttons in padding at its ends, clear of every tab. The newest tab is the one shown and is scrolled into view, so only the button toward the earlier tabs shows. Press it or scroll the strip sideways to reach them.",
    state: stateOf(manyShells),
    preferences: TERMINAL_ONLY,
    steps: [openWebsite],
  },
  {
    id: "shell-panel-exited",
    group: GROUP,
    category: "normal",
    title: "A shell that has exited",
    description:
      "The selected tab reads \"zsh (exited)\", muted beside the running one, and the shell's last screen stays up with the notice over its bottom edge: the shell has exited, with Close terminal. The notice offers no restart. The pane is widened to 560px so both tabs show whole.",
    state: stateOf([shellTerminalOf("sh-1", web, "pnpm dev", 0, { busy: true }), shellTerminalOf("sh-2", web, "zsh", 1, { state: "exited" })]),
    preferences: WIDE_TERMINAL,
    steps: [openWebsite, (ui) => ui.wait(500)],
  },
  {
    id: "shell-panel-stacked",
    group: GROUP,
    category: "normal",
    title: "Terminal stacked under Files",
    description:
      "Files above and Terminal below in one pane, each its own card with its own header, the gap between them holding the grip separator. Drag the separator, or focus it and use the arrow keys, to share the height; double-click resets an even split. Only the Terminal header offers Maximize panel; Files has none. The top bar lists the Terminal toggle before the Files toggle, both pressed, though the Files card stacks above.",
    state: stateOf(severalShells),
    preferences: { panelStacks: { project: { open: ["files", "terminal"], shares: {} } } },
    steps: openWebsiteWithFiles,
  },
  {
    id: "shell-panel-maximized",
    group: GROUP,
    category: "normal",
    title: "Terminal panel maximized",
    description:
      "Files and Terminal are both open and the Terminal is maximized: its card leaves the right pane's column and covers only the agent terminal's area, which is inert beneath it, while the sidebar stays as it was. Files stays docked in the column at its full height, with no separator above it, and both toggles in the top bar stay pressed. The Terminal header offers Restore panel size in place of Maximize; restoring returns the card to the stack under Files unchanged.",
    state: stateOf(severalShells),
    preferences: { panelStacks: { project: { open: ["files", "terminal"], shares: {} } } },
    steps: [...openWebsiteWithFiles, (ui) => ui.pressInPanel("terminal", ui.t("panel.maximize"))],
  },
  {
    id: "shell-panel-maximized-alone",
    group: GROUP,
    category: "normal",
    title: "Terminal panel maximized with Files closed",
    description:
      "The Terminal panel is the only panel and is maximized: the right pane's column folds away, with its width handle, and the card spans over the agent terminal's area to the window's end. Its header offers Restore panel size, and the Files toggle in the top bar is not pressed.",
    state: stateOf(severalShells),
    preferences: TERMINAL_ONLY,
    steps: [openWebsite, (ui) => ui.pressInPanel("terminal", ui.t("panel.maximize"))],
  },
  {
    id: "shell-panel-maximized-files-floating",
    group: GROUP,
    category: "normal",
    title: "Files floated in beside a maximized Terminal",
    description:
      "With the Terminal panel maximized and Files closed, hovering the Files toggle floats the Files card in at the window's end, over the maximized card, the pointer resting on it. The Files toggle is not pressed; pressing it docks Files into the column, the Terminal staying maximized.",
    state: stateOf(severalShells),
    preferences: TERMINAL_ONLY,
    steps: [openWebsite, (ui) => ui.pressInPanel("terminal", ui.t("panel.maximize")), (ui) => ui.floatPane(ui.t("titleBar.files.show"))],
  },
  {
    id: "shell-panel-empty",
    group: GROUP,
    category: "normal",
    title: "Terminal panel with no terminals",
    description:
      "The panel open for a project with no shell: \"No terminals\", the line saying a terminal runs your own shell for this project, and New terminal, which starts one. The header has New terminal but no actions menu, since there is no tab to act on. Opening the panel from the top bar's toggle starts a shell on its own, so this is the panel as it stands when the last tab was closed.",
    state: stateOf([]),
    preferences: TERMINAL_ONLY,
    steps: [openWebsite],
  },
  {
    id: "shell-panel-starting",
    group: GROUP,
    category: "normal",
    title: "Terminal panel starting a shell",
    description:
      "New terminal pressed on the empty panel while the daemon has not yet answered: a spinner and \"Starting terminal…\" take the place of the empty state, which would offer a second start. The header keeps its New terminal, and no tab appears until the shell does.",
    state: stateOf([]),
    preferences: TERMINAL_ONLY,
    shellTerminalStart: "pending",
    steps: [openWebsite, (ui) => ui.press(ui.t("shellTerminal.new"))],
  },
  {
    id: "shell-panel-overlay-small-phone",
    group: GROUP,
    category: "normal",
    title: "Terminal panel over the terminal on a small phone",
    width: 360,
    description:
      "In a window 360px wide or less the Terminal card floats over the agent terminal, and the dimmed strip of the terminal beside it narrows from 48px to 24px to leave the card room: its header fits the Terminal name, the tab with its close mark, New terminal, the actions menu and Close panel with no Maximize panel, which a floating card does not offer. Escape closes the card only outside the shell: typing in the shell leaves it open. The width is fixed because the narrow strip exists only at 360px or less.",
    state: stateOf(oneShell),
    steps: [
      (ui) => ui.press(ui.t("titleBar.sidebar.show")),
      openWebsite,
      (ui) => ui.press(ui.t("titleBar.terminal.show")),
    ],
  },
  {
    id: "shell-panel-floating",
    group: GROUP,
    category: "normal",
    title: "Floating Terminal card",
    description:
      "The Terminal card floated in over the agent terminal by hovering the Terminal toggle with no panel open, the pointer now resting on the card. Only the Terminal card floats, with the same rounded corners, hairline edge and shadow as the floating Files card, over the agent terminal, which shows in the gaps around it. The Terminal toggle is not pressed: pressing it docks the pane.",
    state: stateOf(oneShell),
    preferences: { panelStacks: { project: { open: [], shares: {} } } },
    steps: [openWebsite, (ui) => ui.floatPane(ui.t("titleBar.terminal.show"))],
  },
  {
    id: "shell-panel-header-menu",
    group: GROUP,
    category: "normal",
    title: "Actions menu of the shown tab",
    description:
      "The header's actions button open on the shown tab, named after it: Rename terminal and Close terminal. It is the keyboard's way to a tab's actions, as a tab has no close control the keyboard can reach.",
    state: stateOf(severalShells),
    preferences: TERMINAL_ONLY,
    steps: [openWebsite, (ui) => headerMenu(ui, "git log")],
  },
  {
    id: "shell-panel-tab-menu",
    group: GROUP,
    category: "normal",
    title: "Context menu of a tab",
    description:
      "A right-click on a tab that is not the shown one opens the same two actions at the pointer, for that tab: its context menu is its own, the shown tab does not change. The pane is widened to 560px so all three tabs are whole and the right-clicked one is in plain view.",
    state: stateOf(severalShells),
    preferences: WIDE_TERMINAL,
    steps: [openWebsite, (ui) => ui.contextMenu("pnpm dev")],
  },
  {
    id: "shell-panel-tab-menu-focused",
    group: GROUP,
    category: "normal",
    title: "Context menu of the focused tab, the tab closed elsewhere",
    description:
      "The pnpm dev tab had the keyboard focus when its context menu opened at it, the shown tab unchanged; focus is now inside the open menu, and the menu returns it to the pnpm dev tab when it closes. \"Another window closes pnpm dev\" among the \"Live changes\" buttons removes the tab under the open menu, to show where focus goes when the tab it would return to is gone: the tab fades out, the menu goes with it and focus lands on the shown tab (git log), with its focus ring, rather than on the page. \"Another window reopens pnpm dev\" brings the tab back, not selected. With reduced motion the tab goes at once.",
    state: stateOf(severalShells),
    preferences: WIDE_TERMINAL,
    steps: [openWebsite, (ui) => ui.focusControl("pnpm dev"), (ui) => ui.contextMenu("pnpm dev")],
    liveChanges: [
      terminalClosed("close-pnpm-dev", "Another window closes pnpm dev", "sh-2"),
      terminalOpened("open-pnpm-dev", "Another window reopens pnpm dev", severalShells[1]),
    ],
  },
  {
    id: "shell-rename",
    group: GROUP,
    category: "normal",
    title: "Rename terminal dialog",
    description:
      "The Rename terminal dialog opened from the actions menu, its Name field holding the shell's current title. A name of blanks is refused with \"Name is required.\".",
    state: stateOf(severalShells),
    preferences: TERMINAL_ONLY,
    steps: [openWebsite, (ui) => headerMenu(ui, "git log"), (ui) => ui.press(ui.t("shellTerminal.rename"))],
  },
  {
    id: "shell-close-busy",
    group: GROUP,
    category: "normal",
    title: "Closing a terminal that is busy",
    description:
      "Closing the shown tab, whose shell is running pnpm dev, asks first: a warning modal titled Close terminal whose first sentence names the terminal as a chip, then the warning that closing ends what runs there, and a destructive Close terminal button. An idle or exited tab closes without asking.",
    state: stateOf([shellTerminalOf("sh-1", web, "zsh", 0), shellTerminalOf("sh-2", web, "pnpm dev", 1, { busy: true })]),
    preferences: TERMINAL_ONLY,
    steps: [openWebsite, (ui) => headerMenu(ui, "pnpm dev"), (ui) => ui.press(ui.t("shellTerminal.close"))],
  },
  {
    id: "shell-panel-reconnecting",
    group: GROUP,
    category: "exceptional",
    title: "Shell tab reconnecting",
    description:
      "The shell is running but its terminal socket never connects: after the first failed attempt the tab is covered by the spinner and \"Reconnecting the terminal…\", while the agent terminal beside it is unaffected. The tab strip stays usable.",
    state: stateOf(oneShell),
    preferences: TERMINAL_ONLY,
    shellTerminal: "refuse",
    steps: [openWebsite, (ui) => ui.wait(1500)],
  },
  {
    id: "shell-panel-disconnected",
    group: GROUP,
    category: "exceptional",
    title: "Shell tab disconnected",
    description:
      "The automatic attempts are spent: the tab is covered by the unplugged icon, \"Terminal disconnected\" and a centred Reconnect button, reachable with F6. The agent terminal beside it is unaffected.",
    state: stateOf(oneShell),
    preferences: TERMINAL_ONLY,
    shellTerminal: "refuse",
    terminalRetriesImmediate: true,
    steps: [openWebsite, (ui) => ui.wait(2000)],
  },
];
