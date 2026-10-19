import { Button, Tabs } from "@heroui/react";
import React, { useEffect, useState } from "react";

import { accountsByAgent, AGENT_LABEL, AGENT_OPTIONS } from "../agents";
import { InlineChip } from "../components/InlineChip";
import { AgentTabList } from "../components/AgentTabList";
import { FadeOverflow } from "../components/FadeOverflow";
import { HelpTip } from "../components/HelpTip";
import { PATH_VAR_PARAMS } from "../components/literalParams";
import { MarkedPath } from "../components/MarkedPath";
import { ConfirmDialog } from "../dialogs/ConfirmDialog";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { Account, Agent, ApprovalReviewer, ClaudeApprovalMode } from "../protocol";
import { AccountNameLine, AccountQuotaSummary } from "../quota/AccountQuotaSummary";
import { useAccountQuota } from "../quota/useAccountQuota";
import { useDaemon, useDaemonStore } from "../store";
import { useListMotion } from "../components/useListMotion";
import { AccountDialog } from "./AccountDialog";
import { OptionSelect } from "../dialogs/OptionSelect";
import { errorPresentation } from "../errorPresentation";
import { useSectionRefocus } from "./useSectionRefocus";
import { useStickyHeadingFocus } from "../components/stickyHeadingFocus";

/** The agent strip's height, the `p-1.5` clearance around its tabs and its 1px rule included. The strip is given
 * this height by the `h-[45px]` class below, not left to HeroUI's own sizing, so the two are one number: the
 * Settings dialog clears its fade and the scroll padding for focus moves for this much. */
export const AGENT_STRIP_HEIGHT = 45;

/** Every account, grouped by agent with one tab for each (the first agent's open) and the agent's default account
 * first, each with its plan and quota, and the means to add, edit and remove the ones the user owns (see "Agent
 * accounts" in `docs/product/settings.md`); a new account starts on the agent of the tab in view. The default account
 * is not a record: it is the state of pinning nothing, so it is shown from the agent's availability and has no name or
 * directory actions. Opening the section asks the daemon to refresh the quotas that are due, those of every agent and
 * not only of the tab in view. */
export function AgentAccountsSection(): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  // `undefined` is closed; `null` is the form for a new account.
  const [editing, setEditing] = useState<Account | null | undefined>();
  const [removing, setRemoving] = useState<Account>();
  const [selected, setSelected] = useState<Agent>(AGENT_OPTIONS[0].value);
  // Bumped when a removal has gone through: the Remove button that was pressed goes with its
  // account and the confirmation closes with it, so focus has to be sent back to the section's tab.
  // Cancelling removes no control, so it must not.
  const [removedCount, setRemovedCount] = useState(0);
  useSectionRefocus([accounts, removedCount]);
  const pinnedFocus = useStickyHeadingFocus(AGENT_STRIP_HEIGHT);
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
      <Button size="sm" variant="outline" preventFocusOnPress onPress={() => setEditing(null)}>
        {t("settings.accounts.add")}
      </Button>
      {/* The secondary variant is an underline strip, unlike the filled tabs that switch the
          sections. Its list drops the padding the focus ring needs, so it is given back here, and its underline is
          drawn as a groove like the rows' dividers instead of HeroUI's plain border. The strip pins
          itself to the top of the section's scroller over the rows, opaque on the dialog's own surface and flush
          with the scroller's edge (`--section-padding` is the padding the Settings dialog gives the section). */}
      <Tabs variant="secondary" selectedKey={selected} onSelectionChange={(key) => setSelected(key as Agent)}>
        <AgentTabList
          {...pinnedFocus}
          agents={AGENT_OPTIONS.map(({ value }) => value)}
          label={t("settings.accounts.agents")}
          className={`sticky top-[calc(-1*var(--section-padding))] z-3 h-[45px] groove-b bg-overlay ${pinnedFocus.className}`}
        />
        {accountsByAgent(accounts).map(({ agent, accounts: owned }) => (
          <AgentAccountsPanel key={agent} agent={agent} owned={owned} onEdit={setEditing} onRemove={setRemoving} />
        ))}
      </Tabs>
      {editing !== undefined && <AccountDialog account={editing ?? undefined} agent={selected} onClose={() => setEditing(undefined)} />}
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
function AgentAccountsPanel({ agent, owned, onEdit, onRemove }: {
  agent: Agent;
  owned: Account[];
  onEdit: (account: Account) => void;
  onRemove: (account: Account) => void;
}): React.ReactElement {
  const t = useT();
  const listRef = useListMotion<HTMLDivElement>({ descendants: true });
  const defaultReviewer = useDaemonStore((s) => s.settings.codex_default_approval_reviewer);
  const defaultMode = useDaemonStore((s) => s.settings.claude_default_approval_mode);
  const state = useDaemonStore((s) => s.agentAvailability.get(agent));
  const unavailable = state?.availability === "unavailable";
  const notUsable = unavailable && (
    <div><Message id="settings.accounts.notUsable" params={{ agent: AGENT_LABEL[agent], ...PATH_VAR_PARAMS }} /></div>
  );
  // The strip's own rule is the grid's top edge unless a line of text sits between them.
  const hasApproval = agent === "codex" || agent === "claude";
  const approvalSelect = (account: Account | null) => {
    const name = account?.name ?? t("settings.accounts.defaultName");
    if (agent === "codex") {
      return <ApprovalReviewerSelect account={account?.id ?? null} name={name} reviewer={account ? account.approval_reviewer : defaultReviewer} />;
    }
    if (agent === "claude") {
      return <ClaudeApprovalModeSelect account={account?.id ?? null} name={name} mode={account ? account.approval_mode : defaultMode} />;
    }
    return null;
  };
  const above = unavailable || hasApproval;
  return (
    <Tabs.Panel id={agent} className="px-0 pt-3">
      <div ref={listRef} className="relative">
        {unavailable && <p className="pb-2 text-sm text-app-muted">{t("agents.notInstalled", { agent: AGENT_LABEL[agent] })}</p>}
        {agent === "codex" && <p className="pb-2 text-sm text-app-muted">{t("settings.accounts.approval.codexDescription")}</p>}
        {agent === "claude" && <p className="pb-2 text-sm text-app-muted">{t("settings.accounts.approval.claudeDescription")}</p>}
        <div className={`grid grid-cols-1 ${above ? "groove-t" : ""} ${hasApproval && owned.length > 0 ? "sm:grid-cols-[minmax(0,1fr)_auto_auto]" : "sm:grid-cols-[minmax(0,1fr)_auto]"}`}>
          <AccountRow
            motionKey={`default:${agent}`}
            agent={agent}
            account={null}
            label={t("settings.accounts.defaultName")}
            help={<HelpTip label={t("settings.accounts.defaultHelp")}>{t("settings.accounts.defaultSetup")}</HelpTip>}
            description={state?.default_account_dir && <MarkedPath path={state.default_account_dir} />}
            approval={approvalSelect(null)}
          />
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
            >
              <Button
                size="sm"
                variant="outline"
                preventFocusOnPress
                aria-label={t("settings.accounts.editLabel", { name: isolate(account.name), agent: AGENT_LABEL[agent] })}
                onPress={() => onEdit(account)}
              >
                {t("common.edit")}
              </Button>
              <Button
                size="sm"
                variant="outline"
                preventFocusOnPress
                aria-label={t("settings.accounts.removeLabel", { name: isolate(account.name), agent: AGENT_LABEL[agent] })}
                onPress={() => onRemove(account)}
              >
                {t("common.remove")}
              </Button>
            </AccountRow>
          ))}
        </div>
      </div>
    </Tabs.Panel>
  );
}

/** Subgrid shares control widths across accounts, including the default account without actions.
 * The account's quota takes the row's full width beneath its controls. */
function AccountRow({ agent, account, label, help, description, approval, children, motionKey }: {
  agent: Agent;
  account: string | null;
  label: React.ReactNode;
  /** A help tip that ends the name line, after the plan. */
  help?: React.ReactNode;
  description: React.ReactNode;
  approval?: React.ReactNode;
  children?: React.ReactNode;
  motionKey: string;
}): React.ReactElement {
  const quota = useAccountQuota(agent, account);
  return (
    <div data-list-motion={motionKey} className="col-span-full grid grid-cols-subgrid items-center gap-x-4 gap-y-3 groove-b py-4 last:border-b-0">
      <div className="min-w-0">
        <AccountNameLine name={label} quota={quota} help={help} />
        <div className="text-sm text-app-muted">{description}</div>
      </div>
      {approval && <div className="min-w-0">{approval}</div>}
      {children && <div className="flex flex-wrap items-center gap-2">{children}</div>}
      {quota && <div className="col-span-full"><AccountQuotaSummary quota={quota} /></div>}
    </div>
  );
}


/** What a choice that needs confirming says before it is saved. */
interface ApprovalConfirmation {
  title: string | React.ReactElement;
  message: string;
  confirmLabel: string;
}

/** An account's approval-mode selector: "follow account configuration" plus the agent's own choices, saved when
 * selected. A choice `confirmation` describes is saved only once its confirmation dialog is accepted; cancelling leaves
 * the stored value as it was. */
function ApprovalSelect<T extends string>({ label, value, choices, save, confirmation }: {
  label: string;
  value?: T | null;
  choices: { value: T; label: string }[];
  /** Sends the choice; null follows the account configuration. */
  save: (value: T | null) => Promise<unknown>;
  confirmation?: (value: T) => ApprovalConfirmation | null;
}): React.ReactElement {
  const t = useT();
  const { toastError } = useDaemon();
  const [busy, setBusy] = useState(false);
  const [confirming, setConfirming] = useState<{ value: T } & ApprovalConfirmation>();
  const choose = (picked: T | "inherit") => {
    const asked = picked === "inherit" ? null : confirmation?.(picked);
    if (asked) {
      setConfirming({ value: picked as T, ...asked });
      return;
    }
    setBusy(true);
    void save(picked === "inherit" ? null : picked)
      .catch((err) => toastError(errorPresentation(err as Error)))
      .finally(() => setBusy(false));
  };
  // The trigger must stay focusable while the popover returns keyboard focus after selection.
  return (
    <div className="w-64 max-w-full">
      <OptionSelect<T | "inherit">
        hideLabel
        label={label}
        value={value ?? "inherit"}
        onChange={choose}
        options={[
          { value: "inherit", label: t("settings.accounts.approval.inherit"), isDisabled: busy },
          ...choices.map((choice) => ({ ...choice, isDisabled: busy })),
        ]}
      />
      {confirming && (
        <ConfirmDialog
          title={confirming.title}
          message={confirming.message}
          confirmLabel={confirming.confirmLabel}
          destructive
          onConfirm={async () => {
            await save(confirming.value);
            setConfirming(undefined);
          }}
          onCancel={() => setConfirming(undefined)}
        />
      )}
    </div>
  );
}

/** Codex's reviewer choice for an account, or for the default account when `account` is null. */
function ApprovalReviewerSelect({ account, name, reviewer }: {
  account: string | null;
  name: string;
  reviewer?: ApprovalReviewer | null;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  return (
    <ApprovalSelect<ApprovalReviewer>
      label={t("settings.accounts.approval.label", { name: isolate(name), agent: AGENT_LABEL.codex })}
      value={reviewer}
      choices={[
        { value: "auto_review", label: t("settings.accounts.approval.auto") },
        { value: "user", label: t("settings.accounts.approval.user") },
      ]}
      save={(value) => request({ type: "set_account_approval_reviewer", account, reviewer: value })}
    />
  );
}

/** Claude Code's four choices. Approve everything is saved only after the confirmation dialog: it makes Claude Code run
 * every tool without asking, and Octoboard suppresses Claude Code's own warning for it (see "Agent accounts" in
 * `docs/product/settings.md`). */
function ClaudeApprovalModeSelect({ account, name, mode }: {
  account: string | null;
  name: string;
  mode?: ClaudeApprovalMode | null;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  return (
    <ApprovalSelect<ClaudeApprovalMode>
      label={t("settings.accounts.approval.label", { name: isolate(name), agent: AGENT_LABEL.claude })}
      value={mode}
      choices={[
        { value: "auto", label: t("settings.accounts.approval.claudeAuto") },
        { value: "manual", label: t("settings.accounts.approval.user") },
        { value: "bypass", label: t("settings.accounts.approval.claudeBypass") },
      ]}
      save={(value) => request({ type: "set_account_approval_mode", account, mode: value })}
      confirmation={(value) => value === "bypass" ? {
        title: account === null
          ? t("settings.accounts.approval.bypassTitleDefault")
          : <Message id="settings.accounts.approval.bypassTitle" params={{ name: <InlineChip dir="auto" wrap size="inherit">{name}</InlineChip> }} />,
        message: t("settings.accounts.approval.bypassMessage"),
        confirmLabel: t("settings.accounts.approval.claudeBypass"),
      } : null}
    />
  );
}
