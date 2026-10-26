import { Button } from "../components/Button";
import { Tabs } from "@heroui/react";
import { Bot, LogIn, Pencil, Settings, Trash2, UserPlus, type LucideIcon } from "lucide-react";
import React, { useContext, useEffect, useRef, useState } from "react";

import { AGENT_LABEL, defaultAccountName, isAgentUnavailable } from "../agents";
import { ICON_TRIGGER_CLASS, takeMenuFocusToRestore, type ActionMenuEntry } from "../components/ActionMenu";
import { AgentTabList } from "../components/AgentTabList";
import { EmptyPanel } from "../components/EmptyPanel";
import { TitledControl } from "../components/TitledControl";
import { InlineReference } from "../components/InlineReference";
import { PATH_VAR_PARAMS } from "../components/literalParams";
import { useStickyHeadingFocus } from "../components/stickyHeadingFocus";
import { ConfirmDialog } from "../dialogs/ConfirmDialog";
import { Message, useT } from "../i18n/react";
import type { Account, Agent } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { ringForAutomaticFocus } from "../touchInput";
import { AccountDetailsDialog } from "./AccountDetailsDialog";
import { AccountDialog } from "./AccountDialog";
import { AutoSelectAccountSetting } from "./AutoSelectAccountSetting";
import { accountLoginOf, loginIsActive } from "./AccountLogin";
import { type ReturnLoginFocus } from "./AccountLoginButton";
import { AccountLoginDialog } from "./AccountLoginDialog";
import { AccountsTable, AccountTableRow, rowActionsTrigger } from "./AccountsTable";
import { AddAgentDialog } from "./AddAgentDialog";
import { AgentApprovalSetting } from "./AgentApprovalSetting";
import { AccountsNavigationContext } from "./AccountsNavigationContext";
import { AgentTabActions, AgentTabMenu, RemoveAgentDialog, useAgentList } from "./AgentListMenu";
import { StartQuotaWindowsSetting } from "./StartQuotaWindowsSetting";
import { SettingsFocusContext, useSectionRefocus } from "./useSectionRefocus";

/** The agent strip's height, the `p-1.5` clearance around its tabs and its 1px rule included. The strip is given
 * this height by the `h-[45px]` class below, not left to HeroUI's own sizing, so the two are one number: the
 * Settings dialog clears its fade and the scroll padding for focus moves for this much. */
export const AGENT_STRIP_HEIGHT = 45;

/** Puts focus on the actions menu button of the table row of `row` (an account id, or `default:<agent>`), once the dialog
 * that was open over it has gone and the menu's own restore of focus has run. */
function returnToRowActions(row: string): void {
  requestAnimationFrame(() => requestAnimationFrame(() => {
    const trigger = rowActionsTrigger(row);
    if (!trigger) return;
    // The user did not ask for this move, so the destination shows its ring.
    ringForAutomaticFocus();
    trigger.focus();
  }));
}

/** An icon-only button for the end of the agent strip, named by `label`, which is also its tooltip: the strip carries
 * no room for words. */
function IconAction({ label, icon: Icon, onPress, buttonRef }: { label: string; icon: LucideIcon; onPress: () => void; buttonRef?: React.Ref<HTMLButtonElement> }): React.ReactElement {
  return (
    <TitledControl title={label}>
      <Button ref={buttonRef} variant="ghost" preventFocusOnPress aria-label={label} className={ICON_TRIGGER_CLASS} onPress={onPress}>
        <Icon aria-hidden="true" className="size-4" />
      </Button>
    </TitledControl>
  );
}

/** Every added agent's accounts, with one tab for each in the order of the list (the first agent's open). A tab holds
 * the agent's Permission approval row (not for Grok), which applies to all its accounts, and a table with the agent's
 * default account first, each a row (name and plan, status, usage, the Disable switch) with an actions menu whose
 * first entry opens the account's details in a modal. The section also has the means to add, edit and remove the
 * accounts the user owns, to rename the default account, and to add, remove and reorder the agents (see
 * `docs/product/agent-accounts.md`); a new account starts on the agent of the tab in view. The two switches, one
 * that picks an account by the weekly quota resetting soonest and one that starts a usage window that has not begun
 * counting, sit above that list, once: both apply to every agent, including when none is added. While the daemon has
 * not decided the list every agent is shown, and the agents cannot be removed or reordered yet. The default account
 * is not a record: it is the state of pinning nothing, so it is shown from the agent's availability and can be
 * renamed but not edited or removed. Opening the section asks the daemon to refresh the quotas that are due, those
 * of every agent and not only of the tab in view. */
export function AgentAccountsSection(): React.ReactElement {
  const t = useT();
  const { request, store } = useDaemon();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const added = useDaemonStore((s) => s.settings.added_agents);
  const { agents, tabs, tabRef, reorder, move } = useAgentList(added);
  // `undefined` is closed; `null` is the form for a new account.
  const [editing, setEditing] = useState<Account | null | undefined>();
  const [removing, setRemoving] = useState<Account>();
  // The agent whose default account is being renamed.
  const [renamingDefault, setRenamingDefault] = useState<Agent>();
  const [removingAgent, setRemovingAgent] = useState<Agent>();
  // The account whose details are in the modal; `focusSharing` puts focus on its sharing switch as it opens.
  const [details, setDetails] = useState<{ agent: Agent; account: string | null; focusSharing?: boolean; fromRow?: boolean }>();
  // The account whose login is in the modal; `start` is whether opening it starts the login, which it does not for one
  // found running when the section opened. `returnFocus` is the Log in button's, when one opened it.
  const [loggingIn, setLoggingIn] = useState<{ agent: Agent; account: string | null; start: boolean; returnFocus?: ReturnLoginFocus }>();
  // The Add agent dialog is open when Settings was asked to put it in front, on opening or by a later request,
  // which the section takes up once; closing that one then puts focus on the button that opens it, as no
  // control had it before.
  const accountsNavigation = useContext(AccountsNavigationContext);
  const focusSectionTab = useContext(SettingsFocusContext);
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
    setDetails(undefined);
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
    requestAnimationFrame(() => addAgentButton.current?.focus());
  };
  // A login found running as the section opens (after a reload, or started by another window) is taken up in the
  // modal, where it can be followed or cancelled.
  useEffect(() => {
    const running = store.getState().accountLogins.find((held) => loginIsActive(held));
    if (running) setLoggingIn({ agent: running.agent, account: running.account, start: false });
  }, []);
  const [chosen, setChosen] = useState<Agent>(agents[0] ?? "claude");
  // The tab in view: the chosen one unless its agent has left the list.
  const selected = agents.includes(chosen) ? chosen : agents[0];
  // An account Settings was asked to show: its agent's tab comes forward and its details open with the sharing switch
  // focused.
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
    setDetails({ agent: account.agent, account: account.id, focusSharing: true });
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
  useEffect(() => {
    // The quotas already held are shown whether or not this succeeds, and the daemon's own poller
    // keeps them current, so a failed request has nothing to report.
    void request({ type: "refresh_account_quotas" }).catch(() => {});
  }, [request]);

  return (
    <>
      <p className="pb-2 text-sm text-app-muted">
        <Message id="settings.accounts.description" params={PATH_VAR_PARAMS} />
      </p>
      <AutoSelectAccountSetting />
      <StartQuotaWindowsSetting />
      {selected === undefined ? (
        <EmptyPanel
          icon={Bot}
          message={t("settings.accounts.noAgents")}
          description={t("settings.accounts.noAgentsHint")}
          action={{ label: t("settings.accounts.addAgent"), icon: Bot, onPress: () => setAddingAgent(true) }}
        />
      ) : (
        /* The secondary variant is an underline strip, unlike the filled tabs that switch the
            sections. Its list drops the padding the focus ring needs, so it is given back here, and its underline is
            drawn as a groove like the rows' dividers instead of HeroUI's plain border. The strip pins
            itself to the top of the section's scroller over the rows, opaque on the dialog's own surface and flush
            with the scroller's edge (`--section-padding` is the padding the Settings dialog gives the section). */
        <Tabs variant="secondary" selectedKey={selected} onSelectionChange={(key) => setChosen(key as Agent)}>
          <AgentTabList
            {...pinnedFocus}
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
                onMove={(by) => move(agent, by)}
                onRemove={() => setRemovingAgent(agent)}
              />
            )}
            notInstalledTip={(agent) => added !== null && isAgentUnavailable(availability, agent) && (
              <Message id="settings.accounts.notInstalledTip" params={{ agent: <InlineReference dir="auto" wrap>{AGENT_LABEL[agent]}</InlineReference>, ...PATH_VAR_PARAMS }} />
            )}
            className={`sticky top-[calc(-1*var(--section-padding))] z-3 h-[45px] w-full groove-b bg-overlay ${pinnedFocus.className}`}
          />
          {agents.map((agent) => (
            <AgentAccountsPanel
              key={agent}
              agent={agent}
              owned={accounts.filter((account) => account.agent === agent)}
              onEdit={setEditing}
              onRemove={setRemoving}
              onRenameDefault={setRenamingDefault}
              onDetails={(account) => setDetails({ agent, account, fromRow: true })}
              onLogin={(agent, account, returnFocus) => setLoggingIn({ agent, account, start: true, returnFocus })}
            />
          ))}
        </Tabs>
      )}
      {added !== null && agents.map((agent, index) => tabs[agent] && (
        <AgentTabMenu
          key={agent}
          agent={agent}
          target={tabs[agent]}
          index={index}
          count={agents.length}
          onMove={(by) => move(agent, by)}
          onRemove={() => setRemovingAgent(agent)}
        />
      ))}
      {editing !== undefined && <AccountDialog account={editing ?? undefined} agent={selected ?? "claude"} onClose={() => setEditing(undefined)} />}
      {renamingDefault && <AccountDialog agent={renamingDefault} renameDefault onClose={() => setRenamingDefault(undefined)} />}
      {details && (
        <AccountDetailsDialog
          key={details.account ?? `default:${details.agent}`}
          agent={details.agent}
          account={details.account}
          focusSharing={details.focusSharing}
          onLogin={(returnFocus) => setLoggingIn({ agent: details.agent, account: details.account, start: true, returnFocus })}
          onClose={() => {
            setDetails(undefined);
            // Opened from a row's menu, the dialog closes onto that menu's button, whichever way the menu left focus.
            if (details.fromRow) returnToRowActions(details.account === null ? `default:${details.agent}` : details.account);
          }}
        />
      )}
      {loggingIn && (
        <AccountLoginDialog
          agent={loggingIn.agent}
          account={loggingIn.account}
          start={loggingIn.start}
          onClose={(succeeded, cancelling) => {
            loggingIn.returnFocus?.(succeeded, cancelling);
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
        <ConfirmDialog
          title={t("settings.accounts.removeTitle")}
          subject={<Message id="settings.accounts.removeSubject" params={{ name: <InlineReference dir="auto" wrap>{removing.name}</InlineReference> }} />}
          message={<p><Message id="settings.accounts.removeMessage" params={{ agent: <InlineReference dir="auto" wrap>{AGENT_LABEL[removing.agent]}</InlineReference> }} /></p>}
          confirmLabel={t("common.remove")}
          destructive
          resetKey={removing.id}
          onConfirm={async () => {
            await request({ type: "delete_account", account: removing.id });
            setRemoving(undefined);
            removalDone();
          }}
          onCancel={() => setRemoving(undefined)}
        />
      )}
    </>
  );
}

/** One agent's tab panel: its Permission approval row (not for Grok), which applies to every account of the agent, and
 * the table of the agent's accounts, the default account first. */
function AgentAccountsPanel({ agent, owned, onEdit, onRemove, onRenameDefault, onDetails, onLogin }: {
  agent: Agent;
  owned: Account[];
  onEdit: (account: Account) => void;
  onRemove: (account: Account) => void;
  onRenameDefault: (agent: Agent) => void;
  onDetails: (account: string | null) => void;
  onLogin: (agent: Agent, account: string | null, returnFocus?: ReturnLoginFocus) => void;
}): React.ReactElement {
  const t = useT();
  const logins = useDaemonStore((s) => s.accountLogins);
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  const defaultName = defaultAccountName(t, defaultNames, agent);
  // What an account's menu holds. The default account is not a record: it can be named, not edited or removed. A
  // further action is one more entry here.
  const accountMenu = (account: Account | null): ActionMenuEntry[] => {
    const row = account?.id ?? `default:${agent}`;
    // Whatever the login state, so an account that is logged in can log in again; focus goes back to the menu's button.
    const login = { label: t("settings.accounts.login"), icon: LogIn, disabled: loginIsActive(accountLoginOf(logins, agent, account?.id ?? null)), onClick: () => onLogin(agent, account?.id ?? null, () => returnToRowActions(row)) };
    const details = { label: t("settings.accounts.settingsItem"), icon: Settings, onClick: () => onDetails(account?.id ?? null) };
    if (!account) return [details, { label: t("settings.accounts.renameDefault"), icon: Pencil, onClick: () => onRenameDefault(agent) }, login];
    return [
      details,
      { label: t("common.edit"), icon: Pencil, onClick: () => onEdit(account) },
      login,
      "separator",
      { label: t("common.remove"), icon: Trash2, destructive: true, onClick: () => onRemove(account) },
    ];
  };
  return (
    <Tabs.Panel id={agent} className="px-0 pt-0">
      <AgentApprovalSetting agent={agent} />
      <AccountsTable agent={agent}>
        {[null, ...owned].map((account) => (
          <AccountTableRow
            key={account?.id ?? `default:${agent}`}
            agent={agent}
            account={account}
            name={account?.name ?? defaultName}
            menu={accountMenu(account)}
            onLogin={(returnFocus) => onLogin(agent, account?.id ?? null, returnFocus)}
          />
        ))}
      </AccountsTable>
    </Tabs.Panel>
  );
}
