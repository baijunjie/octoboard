import { DEFAULT_SETTINGS } from "../../store/defaultSettings";
import { accountOf, minutesAgo, sessionOf, SAMPLE, snapshotState, WINDOWS_MACHINE } from "../../fixtures/builders";
import { FIXTURE_AGENTS } from "../../fixtures/fixtureProviders";
import type { Agent, AgentAvailability } from "../../protocol";
import { quotaOf, quotaWindow } from "../../fixtures/quotas";
import { describing, type Step } from "../../fixtures/interact";
import { WINDOWS_HOST } from "../../test/hosts";
import type { Scenario, TrayFlyoutTabFixture } from "../scenario";

const GROUP = "Notification-area flyout";

const { console: console_, web, api } = SAMPLE;

/** What a daemon's menu is built from: one console with the two sample projects, the accounts of two agents and their
 * quota readings. Claude Code's default account is on the Max plan, and Work has used up its five-hour window, which
 * shows its Limit reached tag and grey bars; Codex's default account has both windows. */
const accounts = [accountOf("a-work", "claude", "Work", "C:\\Users\\dev\\.claude-work")];
const readings = [
  quotaOf("claude", null, "max", [quotaWindow("weekly", 5, 3 * 24 * 60 + 4 * 60), quotaWindow("session", 18, 117)]),
  quotaOf("claude", "a-work", "pro", [quotaWindow("session", 100, 42), quotaWindow("weekly", 137, 2 * 24 * 60 + 5 * 60)], { fetched_at: minutesAgo(7) }),
  quotaOf("codex", null, "plus", [quotaWindow("session", 64, 140), quotaWindow("weekly", 31, 5 * 24 * 60 + 2 * 60)]),
];

function stateOf(sessions: ReturnType<typeof sessionOf>[]): Scenario["state"] {
  const state = snapshotState({
    consoles: [console_],
    projects: [web, api],
    sessions,
    ...WINDOWS_MACHINE,
    settings: { ...DEFAULT_SETTINGS, added_agents: ["claude", "codex"], accounts, default_clone_dir: "C:\\Users\\dev\\Projects" },
    accountQuotas: readings,
  });
  return {
    ...state,
    agentAvailability: new Map<Agent, AgentAvailability>(
      FIXTURE_AGENTS.map((agent) => [agent, { agent, availability: agent === "grok" ? "unavailable" : "available", default_account_dir: `C:\\Users\\dev\\.${agent}` }]),
    ),
  };
}

/** This PC: sessions awaiting instructions, and none waiting for the user, so its tab has no hand. */
const thisPc: TrayFlyoutTabFixture = {
  state: stateOf([
    sessionOf("s-web-1", console_.id, web.id, "Fix the summary layout", "working"),
    sessionOf("s-web-2", console_.id, web.id, "Update the dependencies", "idle"),
    sessionOf("s-api-2", console_.id, api.id, "Review the schema change", "idle", { agent: "codex" }),
  ]),
  selectedSessionId: "s-web-2",
  currentConsoleId: console_.id,
};

/** Staging: a session waiting for the user, which raises the hand on its tab. */
const staging: TrayFlyoutTabFixture = {
  remote: "Staging",
  state: stateOf([
    sessionOf("s-api-1", console_.id, api.id, "Add idempotency keys", "waiting_user"),
    sessionOf("s-web-3", console_.id, web.id, "Draft the release notes", "idle"),
  ]),
  selectedSessionId: "s-api-1",
  currentConsoleId: console_.id,
};

/** Build server: every session is working, which the menu does not list. */
const buildServer: TrayFlyoutTabFixture = {
  remote: "Build server",
  state: stateOf([sessionOf("s-web-4", console_.id, web.id, "Run the nightly checks", "working")]),
  currentConsoleId: console_.id,
};

/** Production: its connection is held for the browser, with no window to list sessions from. */
const production: TrayFlyoutTabFixture = { remote: "Production", held: true };

/** Opens Claude Code's quota row in place. The row is named by its account's reading, which begins with the agent's own
 * name. */
const openClaudeUsage: Step = (ui) => ui.press(describing("Claude Code's usage row", (name) => name.includes("Claude Code")));

/** What every scenario of the flyout shares: the Windows host, and this PC's state to say so of the machine (the
 * flyout's page has no daemon behind it, so nothing else reads it). */
const common = { group: GROUP, category: "normal", state: thisPc.state, host: WINDOWS_HOST, desktopApp: true } as const;

export const trayFlyoutScenarios: Scenario[] = [
  {
    ...common,
    id: "tray-flyout",
    title: "Flyout with a tab for each daemon",
    description:
      "The flyout the notification-area icon opens on Windows, 300 to 420px wide at the top left of the window: a strip of tabs, This PC first and then Staging and Build server in connection order, with the underline on the chosen one. Staging carries a warning dot on its tab (a session of it is waiting for the user); This PC does not. Below the strip, This PC's own lines: its sessions under their status heading (Done) with the agent's mark and a row for each agent's usage with its remaining-quota bars at the end and a chevron. A divider, then Check for updates, This PC's Open board, and after another divider Quit completely. The flyout offers no way to connect to a remote daemon: that is the Remote menu's, in the title bar and the jump list. Compare with the macOS menu: the same lines in the same order, the tabs being the macOS menu's tab row.",
    trayFlyout: { tabs: [thisPc, staging, buildServer] },
  },
  {
    ...common,
    id: "tray-flyout-quota-open",
    title: "Flyout with a usage row opened in place",
    description:
      "Claude usage opened in place, as the macOS submenu is, under its row (its chevron now points down) and with the other rows still where they were: the default account's header with its plan and how long ago it was read at the end, its two windows as one meter row (the 5-hour window's bar with the thinner weekly bar beneath it and the weekly name and detail under that), a divider setting the next account off, as in the macOS submenu, then Work, whose header ends in a Limit reached badge, the app's soft warning badge as elsewhere, not a tag in the menu's own colours, and whose bars are grey. Nothing in the opened rows can be chosen. Opening Codex usage closes this one.",
    trayFlyout: { tabs: [thisPc, staging, buildServer] },
    steps: [openClaudeUsage],
  },
  {
    ...common,
    id: "tray-flyout-remote-tab",
    title: "Flyout on a remote daemon's tab",
    description:
      "The flyout opened on Staging, a remote daemon in a window of its own: its tab is the chosen one and keeps its warning dot. The panel is Staging's own: the waiting session under its heading, its sessions awaiting instructions and its usage rows; after the divider, Check for updates, then Staging's Open board and Disconnect, which closes the daemon's window and keeps the application running. This PC's lines are not shown.",
    trayFlyout: { tabs: [thisPc, staging, buildServer], chosen: 1 },
  },
  {
    ...common,
    id: "tray-flyout-browser-tab",
    title: "Flyout on a daemon held for the browser",
    description:
      "Production is connected with no window of the application, its connection held for the system's browser: its tab, the last one, has no session lines or usage rows to list: below Check for updates it has only Open in browser and Disconnect, which releases the connection.",
    trayFlyout: { tabs: [thisPc, staging, production], chosen: 2 },
  },
  {
    ...common,
    id: "tray-flyout-this-pc-only",
    title: "Flyout with no remote daemon connected and an update known",
    description:
      "No remote daemon is connected, so there is no strip of tabs, only This PC's lines; the keyboard focus starts on the first line. The Check for updates line names the newer version a check found instead (Version 2.1.0 available); a download in progress words it the same way, in one line.",
    trayFlyout: { tabs: [thisPc], updateVersion: "2.1.0" },
  },
  {
    ...common,
    id: "tray-flyout-choice-failed",
    category: "exceptional",
    title: "Flyout whose choice could not be handed to the shell",
    description:
      "A line was chosen and the shell could not be told: the flyout stays open, with an error Alert at its top, above the tab strip, which stays until the flyout is next shown. Look at how it fits the flyout's width (its text wraps at the 420px maximum), its margins, and how it sits against the tab strip and the lines below; it has no buttons, as the window is too small for the error dialog. Pressing Esc when the shell cannot close the flyout shows the same Alert under the title Could not close the menu.",
    trayFlyout: { tabs: [thisPc, staging, buildServer], chooseFails: true },
    steps: [(ui) => ui.press(ui.t("settings.updates.check"))],
  },
];
