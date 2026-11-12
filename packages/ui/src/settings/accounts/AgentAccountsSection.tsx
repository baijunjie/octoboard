import { ArrowUp, ArrowDown, Bot, Info, LogIn, Pencil, Plug, Settings2, Trash2, UserPlus, type LucideIcon } from "lucide-react";
import React, { useContext, useEffect, useRef, useState } from "react";

import { defaultAccountName, isAgentUnavailable } from "../../agents/agents";
import { defaultProviderAgent } from "../../agents/providers";
import { ICON_TRIGGER_CLASS } from "../../components/controls/iconTrigger";
import { takeMenuFocusToRestore } from "../../components/focus/menuFocus";
import type { ActionMenuEntry } from "../../components/menus/menuEntries";
import { Button, TabPanel, Tabs } from "../../primitives";
import { AgentTabList, tabActionsTrigger } from "../../components/tabs/AgentTabList";
import { EmptyPanel } from "../../components/panel/EmptyPanel";
import { TitledControl } from "../../components/hint/TitledControl";
import { PATH_VAR_PARAMS } from "../../components/text/literalParams";
import { canTakeFocus } from "../../components/focus/regionFocus";
import { tryEachFrame } from "../../components/focus/tryEachFrame";
import { useStickyHeadingFocus } from "../../components/scroll/stickyHeadingFocus";
import { Message, useT } from "../../i18n/react";
import type { Account, Agent } from "../../protocol";
import { useDaemon, useDaemonStore, useProviders } from "../../store";
import { ProviderName } from "../../components/text/ProviderName";
import { ringForAutomaticFocus } from "../../input/lastInput";
import { AccountDetailsDialog } from "./AccountDetailsDialog";
import { AccountDialog } from "./AccountDialog";
import { AccountMcpDialog } from "./AccountMcpDialog";
import { AccountSettingsDialog } from "./AccountSettingsDialog";
import { accountLoginOf, loginIsActive } from "../../accounts/accountRecords";
import type { ReturnLoginFocus } from "./AccountLoginButton";
import { AccountLoginDialog } from "./AccountLoginDialog";
import { AccountsTable, AccountTableRow, rowActionsTrigger } from "./AccountsTable";
import { AddAgentDialog } from "./AddAgentDialog";
import { AgentSettingsDialog } from "./AgentSettingsDialog";
import { AgentVersionDialog } from "./AgentVersionDialog";
import { AccountsNavigationContext } from "./AccountsNavigationContext";
import { RemoveAccountDialog } from "./RemoveAccountDialog";
import { AgentTabActions, AgentTabMenu, type AgentVersionEntry } from "./AgentMenus";
import { RemoveAgentDialog } from "./RemoveAgentDialog";
import { hasCliUpdate } from "./cliVersion";
import { useAgentList } from "./useAgentList";
import { useAccountOrder } from "./useAccountOrder";
import { useReadsMcpInventory } from "./useAccountMcpInventory";
import { DaemonSwitchSetting } from "../DaemonSwitchSetting";
import { LeadRows, SettingsLead } from "../SettingsLead";
import { SettingsSearchBlock, useSettingsSearchFilter } from "../settingsSearchFilter";
import { SettingsFocusContext, useSectionRefocus } from "../useSectionRefocus";

/** The agent strip's height, the `p-1` clearance around its tabs and its 1px rule included. The strip is given
 * this height by the `h-[41px]` class below, not left to HeroUI's own sizing, so the two are one number: the
 * Settings dialog clears its fade and the scroll padding for focus moves for this much. */
export const AGENT_STRIP_HEIGHT = 41;

/** Which of an account's dialogs is open: its read-only details, its switches, or its connectors and MCP servers. */
type AccountView = "details" | "settings" | "mcp";

/** The switches above the agents' list, which apply to every agent. */
const ACCOUNT_SWITCHES = ["auto_select_account", "auto_switch_on_usage_limit", "auto_start_quota_windows"] as const;

/** How many animation frames (about half a second) `handFocusOnceClosed` waits for a closing dialog to go. */
export const MAX_FRAMES_TO_WAIT_FOR_CLOSE = 30;

/**
 * Puts focus on what `find` returns once the dialog that was open over the section has gone, for a dialog whose own
 * focus restore would not land there. A dialog closed from an event has gone by the next frame, but one closed from an
 * effect (a login that succeeded, an account removed by another window) may render a frame or more later, and until
 * then Settings is inert behind it and takes no focus: so this tries frame by frame, giving up after
 * `MAX_FRAMES_TO_WAIT_FOR_CLOSE`. Coming as the dialog goes, it runs ahead of React Aria's restore, which then finds
 * focus off `<body>` and leaves it; but a dialog that a menu item opened gives focus back where that menu chose a turn
 * after it unmounts, which would undo the move, so the move is made again after that if focus has left the target. The
 * user did not aim focus there, so by default the ring follows the last input; with `ring` false the move is a restore
 * of what the user opened the dialog from, and the ring follows the input that closed it. Returns what drops it while
 * it still waits; the second try after it lands still runs, and does nothing once the target has left the document.
 */
function handFocusOnceClosed(find: () => HTMLElement | null | undefined, { ring = true } = {}): () => void {
  return tryEachFrame(() => {
    const target = find();
    if (!canTakeFocus(target)) return false;
    if (ring) ringForAutomaticFocus();
    target.focus();
    setTimeout(() => {
      if (document.activeElement !== target && canTakeFocus(target)) target.focus();
    }, 0);
    return true;
  }, MAX_FRAMES_TO_WAIT_FOR_CLOSE);
}

/** An icon-only button for the end of the agent strip, named by `label`, which is also its tooltip: the strip carries
 * no room for words. */
function IconAction({ label, icon: Icon, onPress, buttonRef }: { label: string; icon: LucideIcon; onPress: () => void; buttonRef?: React.Ref<HTMLButtonElement> }): React.ReactElement {
  return (
    <TitledControl title={label}>
      <Button ref={buttonRef} variant="ghost" focusOnPress={false} label={label} className={ICON_TRIGGER_CLASS} onPress={onPress}>
        <Icon aria-hidden="true" className="size-4" />
      </Button>
    </TitledControl>
  );
}

/** Every added agent's accounts, with one tab for each in the order of the list (the first agent's
 * open). A tab holds a table with the agent's accounts in their saved order, each a row
 * (name, plan and email, usage, status, the Disable switch) with an actions menu whose first
 * entries open the account's details and its settings and, for a provider that can list them, its connectors and
 * MCP servers, each in a modal. The agent's own settings, which apply to all
 * its accounts (Tags and the rows its descriptor declares), are in a modal opened by the Settings entry of the
 * tab's actions menu, and the version of its CLI, with the update, is in another opened by the Version entry (none for
 * an agent that is not installed); while an update is available the tab's actions button carries a badge and the
 * entry says so. The section also has the
 * means to add, edit and remove the accounts the user owns, to rename the default account, and to
 * add, remove and reorder the agents (see `docs/product/agent-accounts.md`, and
 * `docs/product/removing-accounts.md` for removal); a new account starts on the agent of the tab in
 * view. The three switches, one that picks a new session's account by the weekly quota resetting
 * soonest, one that switches a session stopped at its account's usage limit to another account, and
 * one that starts a usage window that has not begun counting, sit above that list, once: all apply
 * to every agent, including when none is added. While the daemon has not decided the list every
 * agent is shown, and the agents cannot be removed or reordered yet. The default account is not a
 * record: it is the state of pinning nothing, so it is shown from the agent's availability and can
 * be renamed but not edited or removed. Opening the section asks the daemon to refresh the quotas
 * that are due, those of every agent and not only of the tab in view, and to check the versions of their CLIs. */
export function AgentAccountsSection(): React.ReactElement {
  const t = useT();
  const search = useSettingsSearchFilter();
  const { request, store } = useDaemon();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const added = useDaemonStore((s) => s.settings.added_agents);
  const providers = useProviders();
  const { agents, tabs, tabRef, reorder, move } = useAgentList(added);
  // `undefined` is closed; `null` is the form for a new account.
  const [editing, setEditing] = useState<Account | null | undefined>();
  const [removing, setRemoving] = useState<Account>();
  // The agent whose default account is being renamed.
  const [renamingDefault, setRenamingDefault] = useState<Agent>();
  const [removingAgent, setRemovingAgent] = useState<Agent>();
  // The agent whose settings or version are in the modal, `view` naming which, and which of its menus opened it: the
  // tab's actions button or the tab's own context menu. Closing hands focus back to that control, as a menu that a
  // pointer opened leaves focus elsewhere.
  const [agentDialog, setAgentDialog] = useState<{ view: "settings" | "version"; agent: Agent; from: "actions" | "tab" }>();
  // The account whose details, settings or connectors and MCP are in the modal, whichever `view` names; `focusSharing`
  // puts focus on the sharing switch of its settings as they open.
  const [accountDialog, setAccountDialog] = useState<{ view: AccountView; agent: Agent; account: string | null; focusSharing?: boolean; fromRow?: boolean }>();
  // The account whose login is in the modal; `start` is whether opening it starts the login, which it does not for one
  // found running when the section opened. `returnFocus` is the Log in button's, when one opened it.
  const [loggingIn, setLoggingIn] = useState<{ agent: Agent; account: string | null; start: boolean; returnFocus?: ReturnLoginFocus }>();
  const accountsNavigation = useContext(AccountsNavigationContext);
  const focusSectionTab = useContext(SettingsFocusContext);
  // A handoff still waiting is dropped when another starts or the section unmounts: `find` looks the target up in the
  // whole document, so one left running would put focus on the same row of the next section to mount.
  const [handoff] = useState(() => {
    let cancel: (() => void) | undefined;
    return {
      start: (find: () => HTMLElement | null | undefined, options?: { ring?: boolean }) => {
        cancel?.();
        cancel = handFocusOnceClosed(find, options);
      },
      stop: () => cancel?.(),
    };
  });
  useEffect(() => handoff.stop, [handoff]);
  const closeAgentDialog = (automatic: boolean) => {
    if (!agentDialog) return;
    const { agent, from } = agentDialog;
    setAgentDialog(undefined);
    // A close the user made restores the control the user opened the dialog from, not a handoff: the ring follows the
    // input that closed it. One made by a change the user did not make (the agent removed, its CLI gone) hands focus
    // over, the ring following the last input. When the agent has left the list, with its tab and button, a detached
    // control cannot take it and the section's tab does.
    handoff.start(() => [from === "actions" ? tabActionsTrigger(document, agent) : tabs[agent], tabs[agent]].find(canTakeFocus) ?? focusSectionTab(), { ring: automatic });
  };
  // For a dialog the section opens by itself — on Settings' request, or for a login found running as it opens — which
  // has no control to close onto: React Aria gives focus back to what held it as the dialog mounted, and Settings has
  // not focused anything of its own by then (its tabs attach a render later), so focus would end on Settings' own
  // element, or on `<body>` after a press without a pointer opened Settings, where Escape no longer reaches it. The
  // section's tab takes it instead, where Settings opens.
  const returnToSectionTab = () => handoff.start(focusSectionTab);
  // The Add agent dialog is open when Settings was asked to put it in front, on opening or by a later request,
  // which the section takes up once; closing that one then puts focus on the button that opens it, as no
  // control had it before: the agent strip's Add agent button, or the empty panel's while no agent is added (the two
  // are never shown together, so `addAgentButton` holds whichever is there).
  const openedWithAddAgent = useRef(false);
  // Opened after the first render rather than with it: both dialogs are portalled to the end of `<body>` on the
  // same stacking level, so the later one sits on top, and this one must come after Settings' own.
  const [addingAgent, setAddingAgent] = useState(false);
  // Navigation dismisses account forms so its destination is not left behind a modal; the login dialog keeps
  // its lifecycle.
  const lastDismiss = useRef(accountsNavigation.dismissForms);
  useEffect(() => {
    if (accountsNavigation.dismissForms === lastDismiss.current) return;
    lastDismiss.current = accountsNavigation.dismissForms;
    if (!accountsNavigation.pending) {
      openedWithAddAgent.current = false;
      setAddingAgent(false);
    }
    setEditing(undefined);
    setRemoving(undefined);
    setRenamingDefault(undefined);
    setRemovingAgent(undefined);
    setAgentDialog(undefined);
    setAccountDialog(undefined);
  }, [accountsNavigation.dismissForms, accountsNavigation.pending]);
  useEffect(() => {
    if (!accountsNavigation.pending) return;
    accountsNavigation.consume();
    openedWithAddAgent.current = true;
    setAddingAgent(true);
  }, [accountsNavigation.pending]);
  const addAgentButton = useRef<HTMLButtonElement>(null);
  const closeAddAgent = () => {
    setAddingAgent(false);
    if (!openedWithAddAgent.current) return;
    openedWithAddAgent.current = false;
    handoff.start(() => addAgentButton.current);
  };
  // A login found running as the section opens (after a reload, or started by another window) is taken up in the
  // modal, where it can be followed or cancelled.
  useEffect(() => {
    const running = store.getState().accountLogins.find((held) => loginIsActive(held));
    if (running) setLoggingIn({ agent: running.agent, account: running.account, start: false });
  }, []);
  const [chosen, setChosen] = useState<Agent>(agents[0] ?? defaultProviderAgent(providers));
  // The tab in view: the chosen one unless its agent has left the list.
  const selected = agents.includes(chosen) ? chosen : agents[0];
  // An account Settings was asked to show: its agent's tab comes forward and its settings open with the sharing
  // switch focused.
  useEffect(() => {
    const id = accountsNavigation.account;
    if (id === undefined) return;
    accountsNavigation.consumeAccount();
    const account = accounts.find((held) => held.id === id);
    if (!account || !agents.includes(account.agent)) {
      // Nothing to show, and the dialog skipped the section's tab for this request: the tab takes focus, as for any
      // other navigation, a frame on because the tab may not have reached the dialog's ref as the section mounts.
      requestAnimationFrame(() => focusSectionTab()?.focus());
      return;
    }
    setChosen(account.agent);
    setAccountDialog({ view: "settings", agent: account.agent, account: account.id, focusSharing: true });
  }, [accountsNavigation.account]);
  // Bumped when a removal has gone through: the menu item that was chosen goes with its account or agent and the
  // confirmation closes with it, so focus has to be sent back to the section's tab. Cancelling removes no control,
  // so it must not.
  const [removedCount, setRemovedCount] = useState(0);
  useSectionRefocus([accounts, removedCount, added]);
  const removalDone = () => {
    // The pointer-opened menu that asked for the removal left the confirmation the element that held focus when the
    // menu opened, to hand focus back to as it closes, a moment after the section's tab has taken it. A removal ends
    // on the section's tab whatever held focus before (another section's tab, when an accessibility press selected
    // this section without moving focus), so that hand-back is dropped.
    takeMenuFocusToRestore();
    setRemovedCount((count) => count + 1);
  };
  const pinnedFocus = useStickyHeadingFocus(AGENT_STRIP_HEIGHT);
  const availability = useDaemonStore((s) => s.agentAvailability);
  const cliStatuses = useDaemonStore((s) => s.agentCliStatuses);
  // The Version entry of an agent's menus: none for an agent that is not installed.
  const versionEntry = (agent: Agent): AgentVersionEntry | undefined => isAgentUnavailable(availability, agent) ? undefined : { update: hasCliUpdate(cliStatuses.get(agent)), installed: cliStatuses.get(agent)?.installed_version ?? null };
  useEffect(() => {
    // The quotas already held are shown whether or not this succeeds, and the daemon's own poller
    // keeps them current, so a failed request has nothing to report.
    void request({ type: "refresh_account_quotas" }).catch(() => {});
  }, [request]);
  // The versions of the installed CLIs, which the daemon checks only when asked: on opening, and again after each fresh
  // snapshot (a reconnect). The statuses held are shown whether or not this succeeds, and the daemon holds its own
  // floor between checks.
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  useEffect(() => {
    void request({ type: "check_agent_clis" }).catch(() => {});
  }, [request, snapshotEpoch]);

  const renderAccountDialog = (open: NonNullable<typeof accountDialog>) => {
    const closeAccountDialog = () => {
      setAccountDialog(undefined);
      // Opened from a row's menu, the dialog closes onto that menu's button, whichever way the menu left focus, or
      // onto the section's tab when the row has gone with its account.
      const row = open.account === null ? `default:${open.agent}` : open.account;
      if (open.fromRow) handoff.start(() => rowActionsTrigger(row) ?? focusSectionTab());
      else returnToSectionTab();
    };
    const key = `${open.view}:${open.account ?? `default:${open.agent}`}`;
    if (open.view === "details") return <AccountDetailsDialog key={key} agent={open.agent} account={open.account} onClose={closeAccountDialog} />;
    if (open.view === "mcp") return <AccountMcpDialog key={key} agent={open.agent} account={open.account} onClose={closeAccountDialog} />;
    return <AccountSettingsDialog key={key} agent={open.agent} account={open.account} focusSharing={open.focusSharing} onClose={closeAccountDialog} />;
  };

  return (
    <>
      <SettingsLead>
        <Message id="settings.accounts.description" params={PATH_VAR_PARAMS} />
      </SettingsLead>
      {/* The rule under the switches parts them from the agents, so it goes while a search hides the agents. */}
      <LeadRows ruledBelow={search.shows("accounts.agents")} hidden={!ACCOUNT_SWITCHES.some(search.shows)}>
        {ACCOUNT_SWITCHES.map((field) => <DaemonSwitchSetting key={field} field={field} />)}
      </LeadRows>
      <SettingsSearchBlock id="accounts.agents">
        {selected === undefined ? (
          <EmptyPanel
            icon={Bot}
            message={t("settings.accounts.noAgents")}
            description={t("settings.accounts.noAgentsHint")}
            action={{ label: t("settings.accounts.addAgent"), icon: Bot, onPress: () => setAddingAgent(true) }}
            actionRef={addAgentButton}
          />
        ) : (
          /* This is the underline strip, unlike the filled tabs that switch the sections. Its list drops the padding
              the focus ring needs, so it is given back here, and its underline is drawn as a groove like the rows'
              dividers instead of HeroUI's plain border. The strip pins itself to the top of the section's scroller
              over the rows, opaque on the dialog's own surface and flush with the scroller's edge
              (`--section-padding` is the padding the Settings dialog gives the section). */
          <Tabs variant="underline" className="gap-0" selectedKey={selected} onSelectionChange={(key) => setChosen(key as Agent)}>
            <AgentTabList
              {...pinnedFocus}
              providers={providers}
              agents={agents}
              label={t("settings.accounts.agents")}
              tabRef={added === null ? undefined : tabRef}
              onReorder={added === null ? undefined : reorder}
              trailingWidth="pe-[4.75rem]"
              trailing={
                <div className="flex items-center gap-1">
                  <IconAction label={t("settings.accounts.addAgent")} icon={Bot} buttonRef={addAgentButton} onPress={() => setAddingAgent(true)} />
                  <IconAction label={t("settings.accounts.add")} icon={UserPlus} onPress={() => setEditing(null)} />
                </div>
              }
              tabActions={added === null ? undefined : (agent) => (
                <AgentTabActions
                  agent={agent}
                  index={agents.indexOf(agent)}
                  count={agents.length}
                  version={versionEntry(agent)}
                  onSettings={() => setAgentDialog({ view: "settings", agent, from: "actions" })}
                  onVersion={() => setAgentDialog({ view: "version", agent, from: "actions" })}
                  onMove={(by) => move(agent, by)}
                  onRemove={() => setRemovingAgent(agent)}
                />
              )}
              notInstalledTip={(agent) => added !== null && isAgentUnavailable(availability, agent) && (
                <Message id="settings.accounts.notInstalledTip" params={{ agent: <ProviderName agent={agent} />, ...PATH_VAR_PARAMS }} />
              )}
              className={`sticky top-[calc(-1*var(--section-padding))] z-3 h-[41px] w-full groove-b bg-overlay ${pinnedFocus.className}`}
            />
            {agents.map((agent) => (
              <AgentAccountsPanel
                key={agent}
                agent={agent}
                owned={accounts.filter((account) => account.agent === agent)}
                onEdit={setEditing}
                onRemove={setRemoving}
                onRenameDefault={setRenamingDefault}
                onOpenAccount={(view, account) => setAccountDialog({ view, agent, account, fromRow: true })}
                onLogin={(agent, account, returnFocus) => setLoggingIn({ agent, account, start: true, returnFocus })}
                handOff={handoff.start}
              />
            ))}
          </Tabs>
        )}
      </SettingsSearchBlock>
      {added !== null && agents.map((agent, index) => tabs[agent] && (
        <AgentTabMenu
          key={agent}
          agent={agent}
          target={tabs[agent]}
          index={index}
          count={agents.length}
          version={versionEntry(agent)}
          onSettings={() => setAgentDialog({ view: "settings", agent, from: "tab" })}
          onVersion={() => setAgentDialog({ view: "version", agent, from: "tab" })}
          onMove={(by) => move(agent, by)}
          onRemove={() => setRemovingAgent(agent)}
        />
      ))}
      {editing !== undefined && <AccountDialog account={editing ?? undefined} agent={selected ?? defaultProviderAgent(providers)} onClose={() => setEditing(undefined)} />}
      {renamingDefault && <AccountDialog agent={renamingDefault} renameDefault onClose={() => setRenamingDefault(undefined)} />}
      {accountDialog && renderAccountDialog(accountDialog)}
      {agentDialog && (agentDialog.view === "settings"
        ? <AgentSettingsDialog agent={agentDialog.agent} onClose={() => closeAgentDialog(false)} onGone={() => closeAgentDialog(true)} />
        : <AgentVersionDialog agent={agentDialog.agent} onClose={() => closeAgentDialog(false)} onGone={() => closeAgentDialog(true)} />)}
      {loggingIn && (
        <AccountLoginDialog
          agent={loggingIn.agent}
          account={loggingIn.account}
          start={loggingIn.start}
          onClose={(succeeded, cancelling) => {
            if (loggingIn.returnFocus) loggingIn.returnFocus(succeeded, cancelling);
            else returnToSectionTab();
            setLoggingIn(undefined);
          }}
        />
      )}
      {addingAgent && <AddAgentDialog onClose={closeAddAgent} />}
      {removingAgent && (
        <RemoveAgentDialog
          agent={removingAgent}
          onRemoved={() => {
            setRemovingAgent(undefined);
            removalDone();
          }}
          onCancel={() => setRemovingAgent(undefined)}
        />
      )}
      {removing && (
        <RemoveAccountDialog
          account={removing}
          onRemoved={() => {
            setRemoving(undefined);
            removalDone();
          }}
          onCancel={() => setRemoving(undefined)}
        />
      )}
    </>
  );
}

/** One agent's tab panel: the table of the agent's accounts in their saved order. */
function AgentAccountsPanel(props: AgentAccountsPanelProps): React.ReactElement {
  const order = useAccountOrder(props.agent, props.owned);
  return (
    <TabPanel id={props.agent} className="mt-0 px-0 pt-0">
      <AgentAccountsPanelContent {...props} order={order} />
    </TabPanel>
  );
}

interface AgentAccountsPanelProps {
  agent: Agent;
  owned: Account[];
  onEdit: (account: Account) => void;
  onRemove: (account: Account) => void;
  onRenameDefault: (agent: Agent) => void;
  onOpenAccount: (view: AccountView, account: string | null) => void;
  onLogin: (agent: Agent, account: string | null, returnFocus?: ReturnLoginFocus) => void;
  /** `handFocusOnceClosed`, held by the section so that a handoff still waiting ends with it. */
  handOff: (find: () => HTMLElement | null | undefined) => void;
}

/** Pending writes stay with the tab's wrapper; drag cleanup follows the panel's mounted content instead. */
function AgentAccountsPanelContent({ agent, onEdit, onRemove, onRenameDefault, onOpenAccount, onLogin, handOff, order }: AgentAccountsPanelProps & { order: ReturnType<typeof useAccountOrder> }): React.ReactElement {
  const t = useT();
  const logins = useDaemonStore((s) => s.accountLogins);
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  const defaultName = defaultAccountName(t, defaultNames, agent);
  const readsMcp = useReadsMcpInventory(agent);
  const { stopDrag } = order;
  useEffect(() => stopDrag, [stopDrag]);
  // What an account's menu holds. The default account is not a record: it can be named, not edited or removed. A
  // further action is one more entry here.
  const accountMenu = (account: Account | null): ActionMenuEntry[] => {
    const row = account?.id ?? `default:${agent}`;
    // Whatever the login state, so an account that is logged in can log in again; focus goes back to the menu's button.
    const login = { label: t("settings.accounts.login"), icon: LogIn, disabled: loginIsActive(accountLoginOf(logins, agent, account?.id ?? null)), onClick: () => onLogin(agent, account?.id ?? null, () => handOff(() => rowActionsTrigger(row))) };
    const details = { label: t("settings.accounts.detailsItem"), icon: Info, onClick: () => onOpenAccount("details", account?.id ?? null) };
    const settings = { label: t("settings.accounts.settingsItem"), icon: Settings2, onClick: () => onOpenAccount("settings", account?.id ?? null) };
    // Only for a provider that can list an account's connectors and MCP servers.
    const mcp: ActionMenuEntry[] = readsMcp ? [{ label: t("settings.accounts.mcp.title"), icon: Plug, onClick: () => onOpenAccount("mcp", account?.id ?? null) }] : [];
    const index = order.ids.indexOf(account?.id ?? null);
    const moves: ActionMenuEntry[] = [
      "separator",
      { label: t("settings.accounts.moveEarlier"), icon: ArrowUp, disabled: order.busy || index <= 0, onClick: () => order.move(account?.id ?? null, -1) },
      { label: t("settings.accounts.moveLater"), icon: ArrowDown, disabled: order.busy || index >= order.ids.length - 1, onClick: () => order.move(account?.id ?? null, 1) },
    ];
    if (!account) return [details, settings, ...mcp, { label: t("settings.accounts.renameDefault"), icon: Pencil, onClick: () => onRenameDefault(agent) }, login, ...moves];
    return [
      details,
      settings,
      ...mcp,
      { label: t("common.edit"), icon: Pencil, onClick: () => onEdit(account) },
      login,
      ...moves,
      "separator",
      { label: t("common.remove"), icon: Trash2, destructive: true, onClick: () => onRemove(account) },
    ];
  };
  return (
    <>
      <AccountsTable agent={agent}>
        {order.rows.map((account) => (
          <AccountTableRow
            key={account?.id ?? `default:${agent}`}
            agent={agent}
            account={account}
            name={account?.name ?? defaultName}
            menu={accountMenu(account)}
            dragDisabled={order.busy || order.ids.length < 2}
            onDragStart={(event) => order.drag(account?.id ?? null, event)}
            onLogin={(returnFocus) => onLogin(agent, account?.id ?? null, returnFocus)}
          />
        ))}
      </AccountsTable>
    </>
  );
}
