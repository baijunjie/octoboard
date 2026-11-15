// Loaded by `frame.tsx` after `prepareStorage` has run: the app's menu builder reads the session order preference.
import { FIXTURE_PROVIDERS } from "../fixtures/fixtureProviders";
import { translatorFor, type Translate } from "../i18n/catalog";
import type { Language } from "../i18n/languages";
import type { StatusItemMenu, TrayFlyoutLine, TrayFlyoutModel } from "../platform";
import type { Appearance } from "../preferences/cssColour";
import { indexSessions } from "../sessions/sessionIndex";
import { withIcons } from "../statusItem/statusItemIcons";
import { buildStatusItemMenu, statusItemAppLabels } from "../statusItem/statusItemMenu";
import { sessionOrderPreference } from "../store";
import type { TrayFlyoutFixture, TrayFlyoutTabFixture } from "./scenario";

/** One daemon's menu as its window's UI sends it to the shell: the app's own builder over the tab's state. */
async function menuOf(t: Translate, language: Language, appearance: Appearance, tab: TrayFlyoutTabFixture, updatesLabel: string): Promise<StatusItemMenu> {
  const state = tab.state!;
  const consoles = state.consoles ?? new Map();
  const projects = state.projects ?? new Map();
  const sessions = state.sessions ?? new Map();
  const settings = state.settings!;
  const draft = buildStatusItemMenu(t, {
    os: "windows",
    selectedSessionId: tab.selectedSessionId,
    currentConsoleId: tab.currentConsoleId,
    consoles: [...consoles.values()],
    projects: [...projects.values()],
    sessions: [...sessions.values()],
    consoleMap: consoles,
    projectMap: projects,
    delegating: indexSessions(sessions).delegatingIds,
    queuedSwitches: state.queuedAccountSwitches ?? new Set(),
    order: sessionOrderPreference.get(),
    quota: {
      language,
      now: Date.now(),
      accounts: settings.accounts,
      accountOrder: settings.account_order,
      addedAgents: settings.added_agents,
      defaultAccountNames: settings.default_account_names,
      disabledDefaultAccounts: settings.disabled_default_accounts,
      agentAvailability: state.agentAvailability!,
      accountQuotas: state.accountQuotas ?? [],
      providers: state.providers ?? FIXTURE_PROVIDERS,
    },
  });
  draft.updates = updatesLabel;
  return withIcons(draft, appearance);
}

/** A tab's lines of what it lists, in the flyout's order (`tab_lines` in `apps/desktop/src-tauri/src/tray/tabs.rs`):
 * the sessions, each section after its heading, and the quota submenus; a daemon held for the browser lists nothing. */
function panelOf(menu: StatusItemMenu | undefined): TrayFlyoutLine[] {
  const lines: TrayFlyoutLine[] = [];
  if (!menu) return lines;
  for (const section of menu.sections) {
    lines.push({ kind: "heading", label: section.heading, icon: section.icon ? menu.icons[section.icon] : undefined });
    for (const item of section.items) {
      const icon = item.icon ? menu.icons[item.icon] : undefined;
      lines.push(item.session === undefined ? { kind: "note", label: item.label, icon } : { kind: "item", id: `session:${item.session}`, label: item.label, icon });
    }
    lines.push({ kind: "separator" });
  }
  for (const submenu of menu.submenus) {
    lines.push({ kind: "quota", submenu: { ...submenu, icon: submenu.icon ? menu.icons[submenu.icon] : undefined } });
  }
  if (lines.at(-1)?.kind === "separator") lines.pop();
  return lines;
}

/** What can be done with a tab's daemon (`action_lines` in the same file): Open board and, for a remote daemon,
 * Disconnect; a daemon held for the browser offers opening it there instead of Open board. */
function actionsOf(t: Translate, tab: TrayFlyoutTabFixture, menu: StatusItemMenu | undefined): TrayFlyoutLine[] {
  const disconnect: TrayFlyoutLine = { kind: "item", id: "disconnect", label: t("statusItem.disconnect") };
  if (tab.held || !menu) return [{ kind: "item", id: "open-in-browser", label: t("remoteDaemons.browser.open") }, disconnect];
  return tab.remote === undefined ? [{ kind: "item", id: "open", label: menu.open }] : [{ kind: "item", id: "open", label: menu.open }, disconnect];
}

/** The model the flyout is handed for `fixture`, laid out as the shell does (`flyout_model` and `compose`, in
 * `apps/desktop/src-tauri/src/tray/`): a panel of lines for each tab, and the lines every tab shows: Check for updates,
 * the tab's own Open board and Disconnect, and, after a separator, Quit completely. */
export async function trayFlyoutModel(fixture: TrayFlyoutFixture, language: Language, appearance: Appearance): Promise<TrayFlyoutModel> {
  const t = translatorFor(language);
  const labels = statusItemAppLabels(t, "windows");
  const updatesLabel = fixture.updateVersion === undefined ? t("settings.updates.check") : t("statusItem.updateAvailable", { version: fixture.updateVersion });
  const menus = await Promise.all(fixture.tabs.map((tab) => (tab.held || !tab.state ? undefined : menuOf(t, language, appearance, tab, updatesLabel))));
  const tabs = fixture.tabs.map((tab, at) => ({
    key: `tab-${at}`,
    label: tab.remote ?? labels.this_computer,
    remote: tab.remote !== undefined,
    raised: menus[at]?.attention === true,
  }));
  return {
    tabs,
    chosen: Math.min(fixture.chosen ?? 0, tabs.length - 1),
    panels: fixture.tabs.map((_, at) => panelOf(menus[at])),
    shared: [{ kind: "item", id: "updates", label: updatesLabel }],
    actions: fixture.tabs.map((tab, at) => actionsOf(t, tab, menus[at])),
    footer: [{ kind: "item", id: "quit", label: menus[0]?.quit ?? t("statusItem.quit") }],
  };
}
