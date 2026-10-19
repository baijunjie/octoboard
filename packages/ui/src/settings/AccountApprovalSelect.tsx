import React, { useState } from "react";

import { AGENT_LABEL } from "../agents";
import { InlineChip } from "../components/InlineChip";
import { ConfirmDialog } from "../dialogs/ConfirmDialog";
import { OptionSelect } from "../dialogs/OptionSelect";
import { errorPresentation } from "../errorPresentation";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { ApprovalReviewer, ClaudeApprovalMode } from "../protocol";
import { useDaemon } from "../store";

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
export function ApprovalReviewerSelect({ account, name, reviewer }: {
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
 * every tool without asking, and Octoboard suppresses Claude Code's own warning for it (see "Approval modes" in
 * `docs/product/agent-accounts.md`). */
export function ClaudeApprovalModeSelect({ account, name, mode }: {
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
