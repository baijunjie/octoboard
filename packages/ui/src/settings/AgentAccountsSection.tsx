import { Button, Tabs } from "@heroui/react";
import { Bot, LogIn, Pencil, Trash2, UserPlus } from "lucide-react";
import React, { useContext, useEffect, useRef, useState } from "react";

import { AGENT_LABEL, defaultAccountName, isAgentUnavailable } from "../agents";
import { ActionMenu, type ActionMenuEntry } from "../components/ActionMenu";
import { AgentTabList } from "../components/AgentTabList";
import { EmptyPanel } from "../components/EmptyPanel";
import { FadeOverflow } from "../components/FadeOverflow";
import { HelpTip } from "../components/HelpTip";
import { InlineChip } from "../components/InlineChip";
import { PATH_VAR_PARAMS } from "../components/literalParams";
import { MarkedPath } from "../components/MarkedPath";
import { useListMotion } from "../components/useListMotion";
import { useStickyHeadingFocus } from "../components/stickyHeadingFocus";
import { ConfirmDialog } from "../dialogs/ConfirmDialog";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { Account, AccountLogin as Login, Agent } from "../protocol";
import { AccountNameLine, AccountQuotaSummary } from "../quota/AccountQuotaSummary";
import { isNotLoggedIn } from "../quota/quotaFormat";
import { useAccountQuota } from "../quota/useAccountQuota";
import { useDaemon, useDaemonStore } from "../store";
import { ApprovalReviewerSelect, ClaudeApprovalModeSelect } from "./AccountApprovalSelect";
import { AccountDialog } from "./AccountDialog";
import { AccountLoginAnnouncement, accountLoginOf, loginIsActive, useClearSucceededLogin } from "./AccountLogin";
import { AccountLoginDialog } from "./AccountLoginDialog";
import { AddAgentDialog } from "./AddAgentDialog";
import { AccountsNavigationContext } from "./AccountsNavigationContext";
import { AgentMoreMenu, AgentTabMenu, RemoveAgentDialog, useAgentList } from "./AgentListMenu";
import { useSectionRefocus } from "./useSectionRefocus";

/** The agent strip's height, the `p-1.5` clearance around its tabs and its 1px rule included. The strip is given
 * this height by the `h-[45px]` class below, not left to HeroUI's own sizing, so the two are one number: the
 * Settings dialog clears its fade and the scroll padding for focus moves for this much. */
export const AGENT_STRIP_HEIGHT = 45;

/** Every added agent's accounts, with one tab for each in the order of the list (the first agent's open) and the
 * agent's default account first, each with its plan and quota, and the means to add, edit and remove the ones the
 * user owns, to rename the default account, and to add, remove and reorder the agents (see
 * `docs/product/agent-accounts.md`); a new account starts on the agent of the tab in view. While the daemon has not decided
 * the list every agent is shown, and the agents cannot be removed or reordered yet. The default account is not a
 * record: it is the state of pinning nothing, so it is shown from the agent's availability and can be renamed but
 * not edited or removed. Opening the section asks the daemon to refresh the quotas that are due, those of every
 * agent and not only of the tab in view. */
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
  // The account whose login is in the modal; `start` is whether opening it starts the login, which it does not for one
  // found running when the section opened.
  const [loggingIn, setLoggingIn] = useState<{ agent: Agent; account: string | null; start: boolean }>();
  // The Add agent dialog is open when Settings was asked to put it in front, on opening or by a later request,
  // which the section takes up once; closing that one then puts focus on the button that opens it, as no
  // control had it before.
  const accountsNavigation = useContext(AccountsNavigationContext);
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
  // Bumped when a removal has gone through: the menu item that was chosen goes with its account or agent and the
  // confirmation closes with it, so focus has to be sent back to the section's tab. Cancelling removes no control,
  // so it must not.
  const [removedCount, setRemovedCount] = useState(0);
  useSectionRefocus([accounts, removedCount, added]);
  const pinnedFocus = useStickyHeadingFocus(AGENT_STRIP_HEIGHT);
  const availability = useDaemonStore((s) => s.agentAvailability);
  useEffect(() => {
    // The quotas already held are shown whether or not this succeeds, and the daemon's own poller
    // keeps them current, so a failed request has nothing to report.
    void request({ type: "refresh_account_quotas" }).catch(() => {});
  }, [request]);

  const selectedIndex = selected === undefined ? -1 : agents.indexOf(selected);
  return (
    <>
      <p className="pb-2 text-sm text-app-muted">
        <Message id="settings.accounts.description" params={PATH_VAR_PARAMS} />
      </p>
      <div className="flex flex-wrap gap-2">
        <Button size="sm" variant="outline" preventFocusOnPress isDisabled={agents.length === 0} onPress={() => setEditing(null)}>
          <UserPlus aria-hidden="true" className="size-4" />
          {t("settings.accounts.add")}
        </Button>
        <Button ref={addAgentButton} size="sm" variant="outline" preventFocusOnPress onPress={() => setAddingAgent(true)}>
          <Bot aria-hidden="true" className="size-4" />
          {t("settings.accounts.addAgent")}
        </Button>
      </div>
      {selected === undefined ? (
        <EmptyPanel icon={Bot} message={t("settings.accounts.noAgents")} description={t("settings.accounts.noAgentsHint")} />
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
            trailing={added !== null && (
              <AgentMoreMenu
                agent={selected}
                index={selectedIndex}
                count={agents.length}
                onMove={(by) => move(selected, by)}
                onRemove={() => setRemovingAgent(selected)}
              />
            )}
            notInstalledTip={(agent) => added !== null && isAgentUnavailable(availability, agent) && (
              <Message id="settings.accounts.notInstalledTip" params={{ agent: AGENT_LABEL[agent], ...PATH_VAR_PARAMS }} />
            )}
            className={`sticky top-[calc(-1*var(--section-padding))] z-3 h-[45px] groove-b bg-overlay ${pinnedFocus.className}`}
          />
          {agents.map((agent) => (
            <AgentAccountsPanel
              key={agent}
              agent={agent}
              owned={accounts.filter((account) => account.agent === agent)}
              onEdit={setEditing}
              onRemove={setRemoving}
              onRenameDefault={setRenamingDefault}
              onLogin={(agent, account) => setLoggingIn({ agent, account, start: true })}
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
      {loggingIn && <AccountLoginDialog {...loggingIn} onClose={() => setLoggingIn(undefined)} />}
      {addingAgent && <AddAgentDialog onClose={closeAddAgent} />}
      {removingAgent && (
        <RemoveAgentDialog
          agent={removingAgent}
          onRemoved={() => {
            setRemovingAgent(undefined);
            setRemovedCount((count) => count + 1);
          }}
          onCancel={() => setRemovingAgent(undefined)}
        />
      )}
      {removing && (
        <ConfirmDialog
          title={<Message id="settings.accounts.removeTitle" params={{ name: <InlineChip dir="auto" wrap size="inherit">{removing.name}</InlineChip> }} />}
          message={t("settings.accounts.removeMessage", { agent: AGENT_LABEL[removing.agent] })}
          confirmLabel={t("common.remove")}
          destructive
          resetKey={removing.id}
          onConfirm={async () => {
            await request({ type: "delete_account", account: removing.id });
            setRemoving(undefined);
            setRemovedCount((count) => count + 1);
          }}
          onCancel={() => setRemoving(undefined)}
        />
      )}
    </>
  );
}

/** One agent's tab panel. It owns its list motion: a panel that mounts or unmounts as the tab changes is a replaced
 * page, which takes none, while rows added or removed within it still animate. */
function AgentAccountsPanel({ agent, owned, onEdit, onRemove, onRenameDefault, onLogin }: {
  agent: Agent;
  owned: Account[];
  onEdit: (account: Account) => void;
  onRemove: (account: Account) => void;
  onRenameDefault: (agent: Agent) => void;
  onLogin: (agent: Agent, account: string | null) => void;
}): React.ReactElement {
  const t = useT();
  const logins = useDaemonStore((s) => s.accountLogins);
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  const defaultName = defaultAccountName(t, defaultNames, agent);
  const listRef = useListMotion<HTMLDivElement>({ descendants: true });
  const defaultReviewer = useDaemonStore((s) => s.settings.codex_default_approval_reviewer);
  const defaultMode = useDaemonStore((s) => s.settings.claude_default_approval_mode);
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const state = agentAvailability.get(agent);
  const notUsable = isAgentUnavailable(agentAvailability, agent) && (
    <div><Message id="settings.accounts.notUsable" params={{ agent: AGENT_LABEL[agent], ...PATH_VAR_PARAMS }} /></div>
  );
  // The strip's own rule is the grid's top edge unless a line of text sits between them.
  const hasApproval = agent === "codex" || agent === "claude";
  // What an account's menu holds. The default account is not a record: it can be named, not edited or removed. A
  // further action is one more entry here.
  const accountMenu = (account: Account | null): ActionMenuEntry[] => {
    const login = { label: t("settings.accounts.login"), icon: LogIn, disabled: loginIsActive(accountLoginOf(logins, agent, account?.id ?? null)), onClick: () => onLogin(agent, account?.id ?? null) };
    if (!account) return [{ label: t("settings.accounts.renameDefault"), icon: Pencil, onClick: () => onRenameDefault(agent) }, login];
    return [
      { label: t("common.edit"), icon: Pencil, onClick: () => onEdit(account) },
      login,
      "separator",
      { label: t("common.remove"), icon: Trash2, destructive: true, onClick: () => onRemove(account) },
    ];
  };
  const loginProps = (account: Account | null) => {
    const login = accountLoginOf(logins, agent, account?.id ?? null);
    return {
      login: login?.status === "succeeded" ? login : undefined,
      loginName: account?.name ?? defaultName,
    };
  };
  const approvalSelect = (account: Account | null) => {
    const name = account?.name ?? defaultName;
    if (agent === "codex") {
      return <ApprovalReviewerSelect account={account?.id ?? null} name={name} reviewer={account ? account.approval_reviewer : defaultReviewer} />;
    }
    if (agent === "claude") {
      return <ClaudeApprovalModeSelect account={account?.id ?? null} name={name} mode={account ? account.approval_mode : defaultMode} />;
    }
    return null;
  };
  return (
    <Tabs.Panel id={agent} className="px-0 pt-3">
      <div ref={listRef} className="relative">
        {agent === "codex" && <p className="pb-2 text-sm text-app-muted">{t("settings.accounts.approval.codexDescription")}</p>}
        {agent === "claude" && <p className="pb-2 text-sm text-app-muted">{t("settings.accounts.approval.claudeDescription")}</p>}
        <div className={`grid grid-cols-1 ${hasApproval ? "groove-t sm:grid-cols-[minmax(0,1fr)_auto_auto]" : "sm:grid-cols-[minmax(0,1fr)_auto]"}`}>
          <AccountRow
            motionKey={`default:${agent}`}
            agent={agent}
            account={null}
            label={<FadeOverflow dir="auto" titleWhenClipped={defaultName}>{defaultName}</FadeOverflow>}
            help={<HelpTip label={t("settings.accounts.defaultHelp")}>{t("settings.accounts.defaultSetup")}</HelpTip>}
            description={state?.default_account_dir && <MarkedPath path={state.default_account_dir} />}
            approval={approvalSelect(null)}
            {...loginProps(null)}
          >
            <ActionMenu
              label={t("settings.accounts.actionsLabel", { name: isolate(defaultName), agent: AGENT_LABEL[agent] })}
              items={accountMenu(null)}
            />
          </AccountRow>
          {owned.map((account) => (
            <AccountRow
              key={account.id}
              motionKey={account.id}
              agent={agent}
              account={account.id}
              label={
                <FadeOverflow dir="auto" titleWhenClipped={account.name}>
                  {account.name}
                </FadeOverflow>
              }
              description={
                <>
                  <MarkedPath path={account.config_dir} />
                  {notUsable}
                </>
              }
              approval={approvalSelect(account)}
              {...loginProps(account)}
            >
              <ActionMenu
                label={t("settings.accounts.actionsLabel", { name: isolate(account.name), agent: AGENT_LABEL[agent] })}
                items={accountMenu(account)}
              />
            </AccountRow>
          ))}
        </div>
      </div>
    </Tabs.Panel>
  );
}

/** Subgrid shares control widths across accounts.
 * The account's quota takes the row's full width beneath its controls. */
function AccountRow({ agent, account, label, help, description, approval, children, motionKey, login, loginName }: {
  agent: Agent;
  account: string | null;
  label: React.ReactNode;
  /** A help tip that ends the name line, after the plan. */
  help?: React.ReactNode;
  description: React.ReactNode;
  approval?: React.ReactNode;
  children?: React.ReactNode;
  motionKey: string;
  /** The account's login once it has succeeded, for the hidden status region to announce; `loginName` names the
   * account in what it says. */
  login?: Login;
  loginName: string;
}): React.ReactElement {
  const quota = useAccountQuota(agent, account);
  useClearSucceededLogin(login);
  return (
    <div data-list-motion={motionKey} className="col-span-full grid grid-cols-subgrid items-center gap-x-4 gap-y-3 groove-b py-4 last:border-b-0">
      <div className="min-w-0">
        <AccountNameLine name={label} quota={quota} help={help} showLoginState />
        <div className="text-sm text-app-muted">{description}</div>
      </div>
      {approval && <div className="min-w-0">{approval}</div>}
      <div className="flex flex-wrap items-center gap-2">{children}</div>
      {quota && !(typeof quota === "object" && isNotLoggedIn(quota.unavailable)) && <div className="col-span-full"><AccountQuotaSummary quota={quota} /></div>}
      {/* The status region is mounted empty, visually hidden and in the accessibility tree, so the text that arrives in
          it when a login succeeds is announced; the account's badge shows the result. */}
      <div className="sr-only">
        <div role="status"><AccountLoginAnnouncement login={login} name={loginName} /></div>
      </div>
    </div>
  );
}
