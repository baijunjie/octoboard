import { Button, Tabs } from "@heroui/react";
import React, { useEffect, useState } from "react";

import { accountsByAgent, AGENT_LABEL, AGENT_OPTIONS } from "../agents";
import { InlineChip } from "../components/InlineChip";
import { AgentIcon } from "../components/AgentIcon";
import { FadeOverflow } from "../components/FadeOverflow";
import { HelpTip } from "../components/HelpTip";
import { PATH_VAR_PARAMS } from "../components/literalParams";
import { MarkedPath } from "../components/MarkedPath";
import { ConfirmDialog } from "../dialogs/ConfirmDialog";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { Account, Agent, ApprovalReviewer } from "../protocol";
import { AccountQuotaSummary, QuotaPlan } from "../quota/AccountQuotaSummary";
import { useAccountQuota } from "../quota/useAccountQuota";
import { useDaemon, useDaemonStore } from "../store";
import { useTabScrollLabels } from "../components/tabScrollLabels";
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
 * accounts" in `docs/product/settings.md`). The default account is not a record: it is the state of pinning nothing,
 * so it is shown from the agent's availability and has no name or directory actions. Opening the section asks the
 * daemon to refresh the quotas that are due, those of every agent and not only of the tab in view. */
export function AgentAccountsSection(): React.ReactElement {
  const t = useT();
  const tabScrollLabels = useTabScrollLabels();
  const { request } = useDaemon();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  // `undefined` is closed; `null` is the form for a new account.
  const [editing, setEditing] = useState<Account | null | undefined>();
  const [removing, setRemoving] = useState<Account>();
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
      <Tabs variant="secondary">
        <Tabs.ListContainer
          {...pinnedFocus}
          className={`sticky top-[calc(-1*var(--section-padding))] z-3 h-[45px] groove-b bg-overlay ${pinnedFocus.className}`}
          render={(props) => <div {...props}>{tabScrollLabels(props.children)}</div>}
        >
          <Tabs.List aria-label={t("settings.accounts.agents")} className="p-1.5">
            {AGENT_OPTIONS.map(({ value: agent }) => (
              <Tabs.Tab key={agent} id={agent} className="scroll-mx-1.5 gap-2 whitespace-nowrap hover:text-foreground hover:opacity-100">
                <AgentIcon agent={agent} />
                {AGENT_LABEL[agent]}
                <Tabs.Indicator className="-bottom-1.5" />
              </Tabs.Tab>
            ))}
          </Tabs.List>
        </Tabs.ListContainer>
        {accountsByAgent(accounts).map(({ agent, accounts: owned }) => (
          <AgentAccountsPanel key={agent} agent={agent} owned={owned} onEdit={setEditing} onRemove={setRemoving} />
        ))}
      </Tabs>
      {editing !== undefined && <AccountDialog account={editing ?? undefined} onClose={() => setEditing(undefined)} />}
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
  const state = useDaemonStore((s) => s.agentAvailability.get(agent));
  const unavailable = state?.availability === "unavailable";
  const notUsable = unavailable && (
    <div><Message id="settings.accounts.notUsable" params={{ agent: AGENT_LABEL[agent], ...PATH_VAR_PARAMS }} /></div>
  );
  // The strip's own rule is the grid's top edge unless a line of text sits between them.
  const above = unavailable || agent === "codex";
  return (
    <Tabs.Panel id={agent} className="px-0 pt-3">
      <div ref={listRef} className="relative">
        {unavailable && <p className="pb-2 text-sm text-app-muted">{t("agents.notInstalled", { agent: AGENT_LABEL[agent] })}</p>}
        {agent === "codex" && <p className="pb-2 text-sm text-app-muted">{t("settings.accounts.approval.description")}</p>}
        <div className={`grid grid-cols-1 ${above ? "groove-t" : ""} ${agent === "codex" && owned.length > 0 ? "sm:grid-cols-[minmax(0,1fr)_auto_auto]" : "sm:grid-cols-[minmax(0,1fr)_auto]"}`}>
          <AccountRow
            motionKey={`default:${agent}`}
            agent={agent}
            account={null}
            label={t("settings.accounts.defaultName")}
            help={<HelpTip label={t("settings.accounts.defaultHelp")}>{t("settings.accounts.defaultSetup")}</HelpTip>}
            description={state?.default_account_dir && <MarkedPath path={state.default_account_dir} />}
            reviewer={agent === "codex" && <ApprovalReviewerSelect account={null} name={t("settings.accounts.defaultName")} reviewer={defaultReviewer} />}
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
              reviewer={agent === "codex" && <ApprovalReviewerSelect account={account.id} name={account.name} reviewer={account.approval_reviewer} />}
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
function AccountRow({ agent, account, label, help, description, reviewer, children, motionKey }: {
  agent: Agent;
  account: string | null;
  label: React.ReactNode;
  /** A help tip that ends the name line, after the plan. */
  help?: React.ReactNode;
  description: React.ReactNode;
  reviewer?: React.ReactNode;
  children?: React.ReactNode;
  motionKey: string;
}): React.ReactElement {
  const quota = useAccountQuota(agent, account);
  return (
    <div data-list-motion={motionKey} className="col-span-full grid grid-cols-subgrid items-center gap-x-4 gap-y-3 groove-b py-4 last:border-b-0">
      <div className="min-w-0">
        <div className="flex items-center gap-2 text-sm font-medium">
          <div className="min-w-0">{label}</div>
          <QuotaPlan quota={quota} />
          {help}
        </div>
        <div className="text-sm text-app-muted">{description}</div>
      </div>
      {reviewer && <div className="min-w-0">{reviewer}</div>}
      {children && <div className="flex flex-wrap items-center gap-2">{children}</div>}
      {quota && <div className="col-span-full"><AccountQuotaSummary quota={quota} /></div>}
    </div>
  );
}


function ApprovalReviewerSelect({ account, name, reviewer }: {
  account: string | null;
  name: string;
  reviewer?: ApprovalReviewer | null;
}): React.ReactElement {
  const t = useT();
  const { request, toastError } = useDaemon();
  const [busy, setBusy] = useState(false);
  const save = (value: ApprovalReviewer | "inherit") => {
    setBusy(true);
    void request({ type: "set_account_approval_reviewer", account, reviewer: value === "inherit" ? null : value })
      .catch((err) => toastError(errorPresentation(err as Error)))
      .finally(() => setBusy(false));
  };
  // The trigger must stay focusable while the popover returns keyboard focus after selection.
  return (
    <div className="w-64 max-w-full">
      <OptionSelect<ApprovalReviewer | "inherit">
        hideLabel
        label={t("settings.accounts.approval.label", { name: isolate(name) })}
        value={reviewer ?? "inherit"}
        onChange={save}
        options={[
          { value: "inherit", label: t("settings.accounts.approval.inherit"), isDisabled: busy },
          { value: "auto_review", label: t("settings.accounts.approval.auto"), isDisabled: busy },
          { value: "user", label: t("settings.accounts.approval.user"), isDisabled: busy },
        ]}
      />
    </div>
  );
}
