import { Button } from "@heroui/react";
import React, { useState } from "react";

import { accountsByAgent, AGENT_LABEL } from "../agents";
import { InlineChip } from "../components/InlineChip";
import { AgentIcon } from "../components/AgentIcon";
import { FadeOverflow } from "../components/FadeOverflow";
import { PATH_VAR_PARAMS } from "../components/literalParams";
import { MarkedPath } from "../components/MarkedPath";
import { ConfirmDialog } from "../dialogs/ConfirmDialog";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { Account, ApprovalReviewer } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { useListMotion } from "../components/useListMotion";
import { AccountDialog } from "./AccountDialog";
import { OptionSelect } from "../dialogs/OptionSelect";
import { errorPresentation } from "../errorPresentation";
import { useSectionRefocus } from "./useSectionRefocus";

/** Every account, grouped by agent with the agent's default account first, and the means to add,
 * edit and remove the ones the user owns (see "Agent accounts" in `docs/product/settings.md`). The
 * default account is not a record: it is the state of pinning nothing, so it is shown from the
 * agent's availability and has no name or directory actions. */
export function AgentAccountsSection(): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const listRef = useListMotion<HTMLDivElement>({ descendants: true });
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const defaultReviewer = useDaemonStore((s) => s.settings.codex_default_approval_reviewer);
  const availability = useDaemonStore((s) => s.agentAvailability);
  // `undefined` is closed; `null` is the form for a new account.
  const [editing, setEditing] = useState<Account | null | undefined>();
  const [removing, setRemoving] = useState<Account>();
  // Bumped when a removal has gone through: the Remove button that was pressed goes with its
  // account and the confirmation closes with it, so focus has to be sent back to the section's tab.
  // Cancelling removes no control, so it must not.
  const [removedCount, setRemovedCount] = useState(0);
  useSectionRefocus([accounts, removedCount]);

  return (
    <>
      <p className="pb-2 text-sm text-app-muted">
        <Message id="settings.accounts.description" params={PATH_VAR_PARAMS} />
      </p>
      <Button size="sm" variant="outline" preventFocusOnPress onPress={() => setEditing(null)}>
        {t("settings.accounts.add")}
      </Button>
      <div ref={listRef} className="relative">
        {accountsByAgent(accounts).map(({ agent, accounts: owned }) => {
          const state = availability.get(agent);
          const unavailable = state?.availability === "unavailable";
          const notUsable = unavailable && (
            <div><Message id="settings.accounts.notUsable" params={{ agent: AGENT_LABEL[agent], ...PATH_VAR_PARAMS }} /></div>
          );
          return (
            <div key={agent} className="pt-6">
              <h3 data-list-motion={`heading:${agent}`} className="flex items-center gap-2 pb-2 text-sm font-semibold">
                <AgentIcon agent={agent} />
                {unavailable ? t("agents.notInstalled", { agent: AGENT_LABEL[agent] }) : AGENT_LABEL[agent]}
              </h3>
              {agent === "codex" && <p className="pb-2 text-sm text-app-muted">{t("settings.accounts.approval.description")}</p>}
              <div className={`grid grid-cols-1 border-t border-separator ${agent === "codex" && owned.length > 0 ? "sm:grid-cols-[minmax(0,1fr)_auto_auto]" : "sm:grid-cols-[minmax(0,1fr)_auto]"}`}>
                <AccountRow
                  motionKey={`default:${agent}`}
                  label={t("settings.accounts.defaultName")}
                  description={
                    <>
                      <div>{t("settings.accounts.defaultSetup")}</div>
                      {state?.default_account_dir && <MarkedPath path={state.default_account_dir} />}
                    </>
                  }
                  reviewer={agent === "codex" && <ApprovalReviewerSelect account={null} name={t("settings.accounts.defaultName")} reviewer={defaultReviewer} />}
                />
                {owned.map((account) => (
                  <AccountRow
                    key={account.id}
                    motionKey={account.id}
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
                      onPress={() => setEditing(account)}
                    >
                      {t("common.edit")}
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      preventFocusOnPress
                      aria-label={t("settings.accounts.removeLabel", { name: isolate(account.name), agent: AGENT_LABEL[agent] })}
                      onPress={() => setRemoving(account)}
                    >
                      {t("common.remove")}
                    </Button>
                  </AccountRow>
                ))}
              </div>
            </div>
          );
        })}
      </div>
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


/** Subgrid shares control widths across accounts, including the default account without actions. */
function AccountRow({ label, description, reviewer, children, motionKey }: {
  label: React.ReactNode;
  description: React.ReactNode;
  reviewer?: React.ReactNode;
  children?: React.ReactNode;
  motionKey: string;
}): React.ReactElement {
  return (
    <div data-list-motion={motionKey} className="col-span-full grid grid-cols-subgrid items-center gap-x-4 gap-y-3 border-b border-separator py-4 last:border-b-0">
      <div className="min-w-0">
        <div className="text-sm font-medium">{label}</div>
        <div className="text-sm text-app-muted">{description}</div>
      </div>
      {reviewer && <div className="min-w-0">{reviewer}</div>}
      {children && <div className="flex flex-wrap items-center gap-2">{children}</div>}
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
