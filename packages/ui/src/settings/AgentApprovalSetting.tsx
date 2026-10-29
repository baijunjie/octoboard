import React, { useState } from "react";

import { AGENT_LABEL } from "../agents";
import { HelpTip } from "../components/HelpTip";
import { InlineReference } from "../components/InlineReference";
import { ConfirmDialog } from "../dialogs/ConfirmDialog";
import { OptionSelect } from "../dialogs/OptionSelect";
import { reportError } from "../reportError";
import type { PlainMessageKey } from "../i18n/catalog";
import { Message, useT } from "../i18n/react";
import type { Agent, ApprovalReviewer, ClaudeApprovalMode } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { SettingRow } from "./SettingRow";

/** The choices an agent has for approving permission requests, in the order its selector lists them after "Follow
 * account configuration", each with the message that words it. The one place that says which agents have a choice
 * (Grok has none) and what each is called. */
const APPROVAL_CHOICES = {
  codex: [
    { value: "auto_review", label: "settings.accounts.approval.auto" },
    { value: "user", label: "settings.accounts.approval.user" },
  ],
  claude: [
    { value: "auto", label: "settings.accounts.approval.claudeAuto" },
    { value: "manual", label: "settings.accounts.approval.user" },
    { value: "bypass", label: "settings.accounts.approval.claudeBypass" },
  ],
} as const satisfies Partial<Record<Agent, readonly { value: string; label: PlainMessageKey }[]>>;

/** Whether the agent has an approval choice: Claude Code and Codex do, Grok does not. */
export function hasApprovalChoice(agent: Agent): agent is keyof typeof APPROVAL_CHOICES {
  return agent in APPROVAL_CHOICES;
}

/** What a choice that needs confirming says before it is saved. */
interface ApprovalConfirmation {
  title: string;
  message: React.ReactNode;
  confirmLabel: string;
}

/** The approval selector: "follow account configuration" plus the agent's own choices, saved when selected. A choice
 * `confirmation` describes is saved only once its confirmation dialog is accepted; cancelling leaves the stored value
 * as it was. */
function ApprovalSelect<T extends string>({ label, value, choices, save, confirmation }: {
  label: string;
  value?: T | null;
  choices: { value: T; label: string }[];
  /** Sends the choice; null follows the account configuration. */
  save: (value: T | null) => Promise<unknown>;
  confirmation?: (value: T) => ApprovalConfirmation | null;
}): React.ReactElement {
  const t = useT();
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
      .catch((err) => reportError(err, undefined, undefined, undefined, t("settings.accounts.approval.saveFailedTitle")))
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
          errorTitle={t("settings.accounts.approval.saveFailedTitle")}
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

/** Codex's reviewer choice, one for every Codex account. */
function ApprovalReviewerSelect(): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const reviewer = useDaemonStore((s) => s.settings.codex_approval_reviewer);
  return (
    <ApprovalSelect<ApprovalReviewer>
      label={t("settings.accounts.approval.label", { agent: AGENT_LABEL.codex })}
      value={reviewer}
      choices={APPROVAL_CHOICES.codex.map(({ value, label }) => ({ value, label: t(label) }))}
      save={(value) => request({ type: "set_codex_approval_reviewer", reviewer: value })}
    />
  );
}

/** Claude Code's four choices. Approve everything is saved only after the confirmation dialog: it makes Claude Code run
 * every tool without asking in every account, and Octoboard suppresses Claude Code's own warning for it (see "Approval
 * modes" in `docs/product/per-agent-settings.md`). */
function ClaudeApprovalModeSelect(): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const mode = useDaemonStore((s) => s.settings.claude_approval_mode);
  const agent = <InlineReference dir="auto" wrap>{AGENT_LABEL.claude}</InlineReference>;
  return (
    <ApprovalSelect<ClaudeApprovalMode>
      label={t("settings.accounts.approval.label", { agent: AGENT_LABEL.claude })}
      value={mode}
      choices={APPROVAL_CHOICES.claude.map(({ value, label }) => ({ value, label: t(label) }))}
      save={(value) => request({ type: "set_claude_approval_mode", mode: value })}
      confirmation={(value) => value === "bypass" ? {
        title: t("settings.accounts.approval.bypassTitle"),
        message: <p><Message id="settings.accounts.approval.bypassMessage" params={{ agent }} /></p>,
        confirmLabel: t("settings.accounts.approval.claudeBypass"),
      } : null}
    />
  );
}

/** An agent's Permission approval row, above its accounts table: one choice that applies to every account of the agent
 * (see "Approval modes" in `docs/product/per-agent-settings.md`). Nothing for an agent with no approval choice
 * (Grok). */
export function AgentApprovalSetting({ agent }: { agent: Agent }): React.ReactElement | null {
  const t = useT();
  if (!hasApprovalChoice(agent)) return null;
  return (
    <SettingRow
      label={t("settings.accounts.approval.heading")}
      help={
        <HelpTip label={t("settings.accounts.approval.helpLabel")}>
          <Message id={agent === "codex" ? "settings.accounts.approval.codexReviewDescription" : "settings.accounts.approval.claudeReviewDescription"} />
        </HelpTip>
      }
      description={t("settings.accounts.approval.description", { agent: AGENT_LABEL[agent] })}
    >
      {agent === "codex" ? <ApprovalReviewerSelect /> : <ClaudeApprovalModeSelect />}
    </SettingRow>
  );
}
