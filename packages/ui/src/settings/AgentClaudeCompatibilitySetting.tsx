import React, { useState } from "react";

import { AGENT_LABEL } from "../agents";
import { HelpTip } from "../components/HelpTip";
import { InlineReference } from "../components/InlineReference";
import { Message, useT } from "../i18n/react";
import { reportError } from "../reportError";
import type { Agent } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { hasClaudeCompatibility } from "./claudeCompatibility";
import { SettingRow, SettingSwitch } from "./SettingRow";

/** An agent's Claude compatibility row, above its accounts table: one switch that applies to every account of the
 * agent (see "Claude compatibility" in `docs/product/per-agent-settings.md`). Nothing for Claude Code, which has no
 * such setting. Shows the value the daemon carries in `settings.claude_compatibility`, which is the one in effect (an
 * unset Codex follows whether Claude Code is added); "on" is only the fallback when the daemon reports none. Saved when
 * toggled, and the switch stays unavailable while the save is unanswered. */
export function AgentClaudeCompatibilitySetting({ agent }: { agent: Agent }): React.ReactElement | null {
  const t = useT();
  const { request } = useDaemon();
  const enabled = useDaemonStore((s) => s.settings.claude_compatibility?.[agent] ?? true);
  const [busy, setBusy] = useState(false);
  if (!hasClaudeCompatibility(agent)) return null;
  const descriptionId = agent === "codex" ? "settings.accounts.claudeCompat.codexDescription" : "settings.accounts.claudeCompat.grokDescription";
  const change = (value: boolean) => {
    setBusy(true);
    void request({ type: "update_settings", claude_compatibility: { [agent]: value } })
      .catch((err) => reportError(
        err,
        undefined,
        undefined,
        undefined,
        t("settings.accounts.claudeCompat.saveFailedTitle"),
      ))
      .finally(() => setBusy(false));
  };
  return (
    <SettingRow
      label={t("settings.accounts.claudeCompat.heading")}
      help={
        <HelpTip label={t("settings.accounts.claudeCompat.helpLabel")}>
          <Message
            id={descriptionId}
            params={{
              agent: <InlineReference dir="auto" wrap>{AGENT_LABEL[agent]}</InlineReference>,
              claudeMd: <InlineReference dir="ltr" monospace wrap>CLAUDE.md</InlineReference>,
            }}
          />
        </HelpTip>
      }
      description={t("settings.accounts.approval.description", { agent: AGENT_LABEL[agent] })}
    >
      <SettingSwitch
        label={t("settings.accounts.claudeCompat.label", { agent: AGENT_LABEL[agent] })}
        isSelected={enabled}
        isUnavailable={busy}
        onChange={change}
      />
    </SettingRow>
  );
}
