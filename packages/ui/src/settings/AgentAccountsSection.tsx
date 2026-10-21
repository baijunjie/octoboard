import { Button } from "../components/Button";
import { Tabs } from "@heroui/react";
import { Bot, LogIn, Pencil, Trash2, UserPlus } from "lucide-react";
import React, { useContext, useEffect, useRef, useState } from "react";
import { setInteractionModality } from "react-aria";

import { AGENT_LABEL, defaultAccountName, isAgentUnavailable } from "../agents";
import { ActionMenu, type ActionMenuEntry } from "../components/ActionMenu";
import { AgentTabList } from "../components/AgentTabList";
import { ExternalLinkIcon } from "../components/ExternalLinkIcon";
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
import { accountQuotaOf } from "../quota/accountQuotaOf";
import { isNotLoggedIn } from "../quota/quotaFormat";
import { useAccountQuota } from "../quota/useAccountQuota";
import { useDaemon, useDaemonStore } from "../store";
import { ApprovalReviewerSelect, ClaudeApprovalModeSelect } from "./AccountApprovalSelect";
import { AccountDialog } from "./AccountDialog";
import { AutoSelectAccountSetting } from "./AutoSelectAccountSetting";
import { AccountLoginAnnouncer, accountLoginOf, loginIsActive, useClearSucceededLogin } from "./AccountLogin";
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
 * `docs/product/agent-accounts.md`); a new account starts on the agent of the tab in view. The switch
 * that picks an account by the weekly quota resetting soonest sits above that list, once: it applies
 * to every agent, including when none is added. While the daemon has not decided the list every agent
 * is shown, and the agents cannot be removed or reordered yet. The default account is not a
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
  // found running when the section opened. `returnFocus` is the row's, when its own Log in button opened it.
  const [loggingIn, setLoggingIn] = useState<{ agent: Agent; account: string | null; start: boolean; returnFocus?: ReturnLoginFocus }>();
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
      <AutoSelectAccountSetting />
      <div className="flex flex-wrap gap-2 pt-3">
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
              onAdd={() => setEditing(null)}
              onEdit={setEditing}
              onRemove={setRemoving}
              onRenameDefault={setRenamingDefault}
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
function AgentAccountsPanel({ agent, owned, onAdd, onEdit, onRemove, onRenameDefault, onLogin }: {
  agent: Agent;
  owned: Account[];
  onAdd: () => void;
  onEdit: (account: Account) => void;
  onRemove: (account: Account) => void;
  onRenameDefault: (agent: Agent) => void;
  onLogin: (agent: Agent, account: string | null, returnFocus?: ReturnLoginFocus) => void;
}): React.ReactElement {
  const t = useT();
  const logins = useDaemonStore((s) => s.accountLogins);
  const quotas = useDaemonStore((s) => s.accountQuotas);
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
  const hasApproval = agent === "codex" || agent === "claude";
  // What an account's menu holds. The default account is not a record: it can be named, not edited or removed. A
  // further action is one more entry here.
  const accountMenu = (account: Account | null): ActionMenuEntry[] => {
    const login = { label: t("settings.accounts.login"), icon: LogIn, disabled: loginIsActive(accountLoginOf(logins, agent, account?.id ?? null)), onClick: () => onLogin(agent, account?.id ?? null) };
    const loginItems = isNotLoggedIn(accountQuotaOf(quotas, agent, account?.id ?? null)?.unavailable ?? null) ? [] : [login];
    if (!account) return [{ label: t("settings.accounts.renameDefault"), icon: Pencil, onClick: () => onRenameDefault(agent) }, ...loginItems];
    return [
      { label: t("common.edit"), icon: Pencil, onClick: () => onEdit(account) },
      ...loginItems,
      "separator",
      { label: t("common.remove"), icon: Trash2, destructive: true, onClick: () => onRemove(account) },
    ];
  };
  const loginProps = (account: Account | null) => {
    const login = accountLoginOf(logins, agent, account?.id ?? null);
    return {
      login: login?.status === "succeeded" ? login : undefined,
      loginName: account?.name ?? defaultName,
      onLogin: (returnFocus: ReturnLoginFocus) => onLogin(agent, account?.id ?? null, returnFocus),
      loginActive: loginIsActive(login),
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
    <Tabs.Panel id={agent} className="px-0 pt-0">
      <div ref={listRef} className="relative">
        <div className={`grid grid-cols-1 gap-x-6 ${hasApproval ? "sm:grid-cols-2" : "sm:grid-cols-[minmax(0,1fr)_auto]"}`}>
          <div className={`col-span-full grid grid-cols-subgrid items-center pb-2 ${hasApproval ? "sm:groove-b" : ""}`}>
            <Button size="sm" variant="outline" className="justify-self-start" preventFocusOnPress onPress={onAdd}>
              <UserPlus aria-hidden="true" className="size-4" />
              {t("settings.accounts.add")}
            </Button>
            {hasApproval && <div className="col-start-2 hidden sm:block"><ApprovalHeading agent={agent} /></div>}
          </div>
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

/** Told, as the login dialog a row's own Log in button opened closes, whether the login succeeded, and the cancel
 * closing it sent, if any. */
type ReturnLoginFocus = (succeeded: boolean, cancelling?: Promise<void>) => void;

/**
 * Where focus goes once the login dialog the row's own Log in button opened has closed, which React Aria's restore
 * alone does not settle: it puts focus back on that button, and the button is not always there to take it. A login
 * that succeeded takes the button away (it is shown only while the account reads as signed out), before or after the
 * restore, so focus goes to the row's actions instead; so it does when a cancelled login turns out to have succeeded
 * first. Closing the dialog otherwise cancels a login still running, and the button is disabled while the daemon has a
 * login running — including one it registers only after the dialog has gone — so the restore finds nothing to focus,
 * or focus drops off the button as it turns disabled. Focus goes to the button once the cancel has been answered and
 * the button is enabled again, or to the actions if it has gone. Either way focus would otherwise end on `<body>`,
 * where VoiceOver reads the whole web area and does not speak the row's status.
 *
 * The move waits a frame past the restore, which runs in the frame after the dialog unmounts, and acts only where the
 * restore left focus on `<body>` or on a button that is going: a restore that landed right, or on another control,
 * is left alone. A restore React Aria defers further for a screen reader's virtual cursor acts only while focus is
 * still on `<body>`, so it does not undo this move.
 */
function useLoginFocusReturn(
  button: React.RefObject<HTMLElement | null>,
  actions: React.RefObject<HTMLElement | null>,
  { active, succeeded }: { active: boolean; succeeded: boolean },
): ReturnLoginFocus {
  const [returning, setReturning] = useState<{ to: "button" | "actions"; cancelling: boolean }>();
  useEffect(() => {
    if (!returning || returning.cancelling || (returning.to === "button" && active)) return;
    let frame = requestAnimationFrame(() => {
      frame = requestAnimationFrame(() => {
        setReturning(undefined);
        const opener = button.current;
        const target = returning.to === "button" && !succeeded && opener ? opener : actions.current?.querySelector<HTMLElement>("button");
        const focused = document.activeElement;
        if (!target || focused === target || (focused !== document.body && !opener?.contains(focused))) return;
        // The user did not ask for this move, so the destination shows its ring.
        setInteractionModality("keyboard");
        target.focus();
      });
    });
    return () => cancelAnimationFrame(frame);
  }, [returning, active, succeeded]);
  return (closedOnSuccess, cancelling) => {
    const next = { to: closedOnSuccess ? "actions" : "button", cancelling: cancelling !== undefined } as const;
    setReturning(next);
    void cancelling?.then(() => setReturning((current) => (current === next ? { ...next, cancelling: false } : current)));
  };
}

function ApprovalHeading({ agent }: { agent: Agent }): React.ReactElement {
  const t = useT();
  return (
    <div className="flex items-center gap-1 text-sm text-app-muted">
      {t("settings.accounts.approval.heading")}
      <HelpTip label={t("settings.accounts.approval.heading")}>
        {t(agent === "codex" ? "settings.accounts.approval.codexDescription" : "settings.accounts.approval.claudeDescription")}
      </HelpTip>
    </div>
  );
}

/** Subgrid shares control widths across accounts.
 * The account's quota takes the row's full width beneath its controls. */
function AccountRow({ agent, account, label, help, description, approval, children, motionKey, login, loginName, onLogin, loginActive }: {
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
  /** Starts the login from the row's own Log in button. */
  onLogin: (returnFocus: ReturnLoginFocus) => void;
  loginActive: boolean;
}): React.ReactElement {
  const t = useT();
  const quota = useAccountQuota(agent, account);
  const notLoggedIn = typeof quota === "object" && isNotLoggedIn(quota.unavailable);
  useClearSucceededLogin(login);
  const actionsRef = useRef<HTMLDivElement>(null);
  const loginRef = useRef<HTMLButtonElement>(null);
  const returnLoginFocus = useLoginFocusReturn(loginRef, actionsRef, { active: loginActive, succeeded: login !== undefined });
  return (
    <div data-list-motion={motionKey} className="col-span-full grid grid-cols-subgrid items-center gap-y-3 groove-b py-4 last:border-b-0">
      <div className="min-w-0">
        <AccountNameLine name={label} quota={quota} help={help} showLoginState>
          {notLoggedIn && <Button ref={loginRef} size="xs" variant="secondary" preventFocusOnPress isDisabled={loginActive} onPress={() => onLogin(returnLoginFocus)}>
            {t("settings.accounts.login")}
            <ExternalLinkIcon />
          </Button>}
        </AccountNameLine>
        <div className="text-sm text-app-muted">{description}</div>
      </div>
      <div className="contents sm:flex sm:items-center sm:gap-4">
        {approval && (
          <div className="min-w-0 sm:flex-1">
            <div className="mb-1 sm:hidden"><ApprovalHeading agent={agent} /></div>
            {approval}
          </div>
        )}
        <div ref={actionsRef} className="flex shrink-0 flex-wrap items-center gap-2">{children}</div>
      </div>
      {quota && !(typeof quota === "object" && isNotLoggedIn(quota.unavailable)) && <div className="col-span-full"><AccountQuotaSummary quota={quota} /></div>}
      <AccountLoginAnnouncer login={login} name={loginName} />
    </div>
  );
}
