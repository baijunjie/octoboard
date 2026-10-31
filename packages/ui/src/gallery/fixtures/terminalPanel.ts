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

/** The Terminal panel alone, with room to compare several complete tab labels. */
const WIDE_TERMINAL: Preferences = { ...TERMINAL_ONLY, asideWidth: 560 };

const openWebsite = (ui: Ui) => ui.press(ui.session("Fix the summary layout"));
/** Brings the Terminal panel in where it is closed, below 782px, as it is open already in a docked window. */
const showTerminal = (ui: Ui) => ui.showPanel("terminal");
/** Opens the session, then lets the Files panel read its folder, which draws nothing until it has. */
const openWebsiteWithFiles = [openWebsite, (ui: Ui) => ui.wait(400)];

/** The tab row's actions button for the shown terminal, named after it. */
const terminalMenu = (ui: Ui, title: string) => ui.press(ui.t("shellTerminal.actions", { title }));

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
      "The Terminal panel alone in the right pane, as the top bar's Terminal toggle (pressed) opens it. Its first row runs only the Terminal name, Maximize panel and Close panel. The selected shell and its always-visible close mark sit on the left of a separate row below; New terminal and the terminal actions menu stay fixed on that row's right. The selected tab is a rounded rectangle with a blue outline and tinted fill. Terminal offers maximize even as the only panel. The shell draws on the card's own terminal background, inset as the agent terminal is. Choose Phone (420px) to see it float over the agent terminal as a card, with a dimmed strip of the terminal beside it, and no Maximize panel, which a floating card does not offer; Escape closes the card only outside the shell: typing in the shell leaves it open. At 360px or less the strip narrows from 48px to 24px to leave the card room, and the first row still shows the Terminal name and panel controls, with the tab and terminal controls kept on the second row. Compare desktop, 420px and 360px with the width controls.",
    state: stateOf(oneShell),
    preferences: TERMINAL_ONLY,
    steps: [openWebsite, (ui) => ui.showPanel("terminal")],
  },
  {
    id: "shell-panel-tabs",
    group: GROUP,
    category: "normal",
    title: "Several tabs",
    description:
      "Three evenly spaced, rounded rectangular tabs in the order the shells were started, the newest selected with a blue outline and tinted fill. Every tab keeps its close mark visible. The pane is widened to 560px so the three tabs fit whole on their own row below the title and panel controls. New terminal and the terminal actions menu stay fixed on the right of that row; press New terminal for another tab, selected at once. The \"Live changes\" buttons make what another window would: its new terminal joins the strip at the end without being selected, and closing the shown one hands the view to a neighbour, and reopening it afterwards brings it back without selecting it, so the view stays on the neighbour. Tabs fade in or out while the ones after them slide into place; with reduced motion they appear and disappear at once.",
    state: stateOf(severalShells),
    preferences: WIDE_TERMINAL,
    steps: [openWebsite, showTerminal],
    liveChanges: [
      terminalOpened("open-htop", "Another window opens htop", shellTerminalOf("sh-4", web, "htop", 3)),
      terminalClosed("close-htop", "Another window closes htop", "sh-4"),
      terminalClosed("close-git-log", "Another window closes git log", "sh-3"),
      terminalOpened("open-git-log", "Another window reopens git log", severalShells[2]),
    ],
  },
  {
    id: "shell-panel-path-titles",
    group: GROUP,
    category: "normal",
    title: "Short directory titles with full paths on hover",
    description:
      "Automatic directory titles show only the last folder: website from a user@host-prefixed absolute path, and src (exited) from a home-relative path. Hover either shortened title to see its full original title, even when the short name fits. The command git log and the manually named ~/keep tab stay unchanged. The pane is widened to 560px. Evenly spaced, rounded rectangular tabs occupy their own row below the always-visible Terminal title and panel controls. The selected ~/keep tab has a blue outline and tinted fill; every tab shows its close mark, and New terminal and the terminal actions menu stay fixed on the right, outside the scrolling strip. Hover a close mark to see its background, then hold the pointer down to see a stronger pressed fill. Compare light and dark themes at desktop, 420px and 360px widths, using the strip's scroll buttons where needed.",
    state: stateOf([
      shellTerminalOf("sh-1", web, "developer@workstation:/Volumes/Projects/website", 0),
      shellTerminalOf("sh-2", web, "~/code/website/src", 1, { state: "exited" }),
      shellTerminalOf("sh-3", web, "git log", 2),
      shellTerminalOf("sh-4", web, "~/keep", 3, { renamed: true }),
    ]),
    preferences: WIDE_TERMINAL,
    steps: [openWebsite, showTerminal],
  },
  {
    id: "shell-panel-many-tabs",
    group: GROUP,
    category: "normal",
    title: "Many tabs, the strip scrolling",
    description:
      "Twelve shells, more than the strip holds: the rounded rectangular tabs scroll on the second row. A scroll button reserves space only at the edge where it appears, clear of every close mark. New terminal and the terminal actions menu stay fixed on the right, outside the scrolling strip. The Terminal title and panel controls stay on the first row, visible at desktop, 420px and 360px widths. Every tab keeps its close mark visible. The newest tab has a blue outline and tinted fill and is scrolled into view, so only the button toward the earlier tabs shows. Compare light and dark themes with the theme control. Press it or scroll the strip sideways to reach them.",
    state: stateOf(manyShells),
    preferences: TERMINAL_ONLY,
    steps: [openWebsite, showTerminal],
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
    steps: [openWebsite, showTerminal, (ui) => ui.wait(500)],
  },
  {
    id: "shell-panel-stacked",
    group: GROUP,
    category: "normal",
    title: "Two panels adapt to the pane width",
    description:
      "Terminal and Files each have their own card and header. The sidebar starts hidden and the pane widened so Terminal and Files stand side by side. Drag the pane width handle: at 530px of content width or more, Terminal fills the first column and Files the second; below 530px they stack, Terminal above Files. While side by side, drag the full-height grip between the columns to change their relative widths; double-click restores equal widths. When the cards stack, that grip disappears, and widening the pane again brings back the chosen proportion. In the stacked layout, drag the height separator, or focus it and use the arrow keys, to share the height; double-click resets an even split. Only Terminal offers Maximize panel. The top bar lists Terminal before Files, both pressed. Choose Narrow (800px) to see them stack; choose Phone (420px) to see a single floating card.",
    state: stateOf(severalShells),
    preferences: { sidebarVisible: false, asideWidth: 720, panelStacks: { project: { open: ["terminal", "files"], shares: {} } } },
    steps: [...openWebsiteWithFiles, showTerminal],
  },
  {
    id: "shell-panel-maximized",
    group: GROUP,
    category: "normal",
    title: "Terminal panel maximized",
    // A floating card has no Maximize panel: it exists only where the right pane docks (from 782px).
    minWidth: 782,
    description:
      "Files and Terminal are both open and the Terminal is maximized: its card leaves the right pane's column and covers only the agent terminal's area, which is inert beneath it, while the sidebar stays as it was. Files stays docked in the column at its full height, with no separator above it, and both toggles in the top bar stay pressed. The Terminal header offers Restore panel size in place of Maximize; restoring returns the card ahead of Files, side by side when the pane is wide enough and stacked above Files otherwise.",
    state: stateOf(severalShells),
    preferences: { panelStacks: { project: { open: ["files", "terminal"], shares: {} } } },
    steps: [...openWebsiteWithFiles, (ui) => ui.pressInPanel("terminal", ui.t("panel.maximize"))],
  },
  {
    id: "shell-panel-maximized-alone",
    group: GROUP,
    category: "normal",
    title: "Terminal panel maximized with Files closed",
    // A floating card has no Maximize panel: it exists only where the right pane docks (from 782px).
    minWidth: 782,
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
    // A floating card has no Maximize panel: it exists only where the right pane docks (from 782px).
    minWidth: 782,
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
      "The panel open for a project with no shell: \"No terminals\", the line saying a terminal runs your own shell for this project, and New terminal, which starts one. The header keeps the Terminal title and panel controls. The second row remains below it with New terminal alone on the right; the terminal actions menu disappears when the last tab closes. Compare desktop, 420px and 360px with the width controls. Opening the panel from the top bar's toggle starts a shell on its own, so this is the panel as it stands when the last tab was closed.",
    state: stateOf(oneShell),
    preferences: TERMINAL_ONLY,
    steps: [openWebsite, showTerminal, ...terminalClosed("close-last-shell", "Close the last terminal", "sh-1").steps],
  },
  {
    id: "shell-panel-starting",
    group: GROUP,
    category: "normal",
    title: "Terminal panel starting a shell",
    description:
      "New terminal pressed on the empty panel while the daemon has not yet answered: a spinner and \"Starting terminal…\" take the place of the empty state, which would offer a second start. The first row keeps the Terminal title and panel controls. The second row keeps New terminal fixed on the right while no tabs have appeared yet.",
    state: stateOf([]),
    preferences: TERMINAL_ONLY,
    shellTerminalStart: "pending",
    steps: [openWebsite, showTerminal, (ui) => ui.press(ui.t("shellTerminal.new"))],
  },
  {
    id: "shell-panel-floating",
    group: GROUP,
    category: "normal",
    title: "Floating Terminal card",
    // Hovering a toggle floats a panel in only where the right pane docks (from 782px); below it a press does.
    minWidth: 782,
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
      "The actions button at the right of the tab row, beside New terminal, is open for the shown tab and named after it: Rename terminal and Close terminal. It is the keyboard's way to a tab's actions, as a tab has no close control the keyboard can reach.",
    state: stateOf(severalShells),
    preferences: TERMINAL_ONLY,
    steps: [openWebsite, showTerminal, (ui) => terminalMenu(ui, "git log")],
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
    steps: [openWebsite, showTerminal, (ui) => ui.contextMenu("pnpm dev")],
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
    steps: [openWebsite, showTerminal, (ui) => ui.focusControl("pnpm dev"), (ui) => ui.contextMenu("pnpm dev")],
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
    steps: [openWebsite, showTerminal, (ui) => terminalMenu(ui, "git log"), (ui) => ui.press(ui.t("shellTerminal.rename"))],
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
    steps: [openWebsite, showTerminal, (ui) => terminalMenu(ui, "pnpm dev"), (ui) => ui.press(ui.t("shellTerminal.close"))],
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
