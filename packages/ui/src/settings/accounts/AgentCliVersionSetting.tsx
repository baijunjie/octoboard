import React, { useId, useRef, useState } from "react";

import { DaemonErrorMessage } from "../../components/alerts/DaemonErrorMessage";
import { StatusAnnouncer } from "../../components/announce/StatusAnnouncer";
import { DiagnosticBlock } from "../../components/alerts/DiagnosticBlock";
import { raiseError } from "../../components/alerts/raiseError";
import { useHandoffAfterRemoval } from "../../components/focus/HandoffOnUnmount";
import { HelpTip } from "../../components/hint/HelpTip";
import { StatusMark } from "../../components/status/StatusMark";
import { providerLabel } from "../../agents/providers";
import { CodedError } from "../../errors/codedError";
import { daemonMessage, NO_RECORDS } from "../../errors/daemonMessage";
import { isolate } from "../../i18n/isolate";
import { useCurrentLanguage, useT } from "../../i18n/react";
import type { Agent, CliInstallMethod } from "../../protocol";
import { trackAgentCliUpdate, useDaemon, useDaemonStore, useProviders } from "../../store";
import { Button } from "../../primitives";
import { SettingRow } from "../SettingRow";
import { hasCliUpdate, isNewerVersion } from "./cliVersion";

const METHOD_MESSAGES = {
  native: "settings.accounts.cli.method.native",
  npm: "settings.accounts.cli.method.npm",
  homebrew: "settings.accounts.cli.method.homebrew",
  unknown: "settings.accounts.cli.method.unknown",
} as const satisfies Record<CliInstallMethod, string>;

/** An agent's Version row, the content of its `AgentVersionDialog`: the version of its CLI that starts a session now,
 * how it was installed, and an Update button where the install method names a command that updates it (see "Agent CLI
 * versions" in `apps/daemon/PROTOCOL.md`). What it shows is the daemon's `AgentCliStatus` from the store, so it holds
 * across closing the dialog and shows an update another window started; the section asks for the check
 * (`check_agent_clis`), and the dialog is offered only for an agent that is installed.
 *
 * - no status yet: a quiet "Checking the version…";
 * - the installed version, with the latest one when it is newer ("Version … is installed. Version … is available.")
 *   or the words that it is up to date; a version unreadable, or one that is no `major.minor.patch`, is said as far as
 *   it can be;
 * - the install method on a line of its own, and for one the daemon cannot tell, the reason there is no Update (the CLI
 *   has to be updated the way it was installed);
 * - a lookup of the latest version that failed and an update that failed are one warning mark after the sentence, its
 *   popover listing each reason: they block nothing;
 * - Update, pending while the update runs, from the press, even if the dialog was closed and opened again. A failure of
 *   an update pressed here raises the error alert (`AgentCliUpdateAlerts`, which outlives this dialog), since the user
 *   asked for it, and the same failure, seen from another window or found when the dialog opens, is the mark alone,
 *   which stays as its lasting state until the CLI is current or the next update starts. A success is said by the
 *   sentence and spoken once by a status region, as is a failure seen from another window. Running sessions keep their
 *   process, which the help tip says. */
export function AgentCliVersionSetting({ agent }: { agent: Agent }): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const { request, store } = useDaemon();
  const providers = useProviders();
  const status = useDaemonStore((s) => s.agentCliStatuses.get(agent));
  // The `started_at` of the update the status held when this window asked for one, until that update has ended: the
  // button is pending from the press until the status holds a newer one.
  const askedFrom = useDaemonStore((s) => s.cliUpdatesStartedHere.get(agent));
  const update = status?.update ?? undefined;
  const running = update?.state === "running" || (askedFrom !== undefined && (update?.started_at ?? null) === askedFrom);
  // The update this view has seen running, and whether this window asked for it: what its end is announced for.
  const [seen, setSeen] = useState<{ startedAt: number; here: boolean }>();
  if (update?.state === "running" && seen?.startedAt !== update.started_at) setSeen({ startedAt: update.started_at, here: askedFrom !== undefined });
  const ended = seen && update && update.started_at === seen.startedAt && update.state !== "running" ? update.state : undefined;
  const markId = useId();
  const updateId = useId();
  const helpTip = useRef<HTMLSpanElement>(null);
  // The update button goes when the update succeeds, and the mark when the failure it shows is gone; focus held on
  // either goes to the help tip, which is always there.
  const handoffFrom = useHandoffAfterRemoval(() => helpTip.current?.querySelector("button"));
  const installed = status?.installed_version ?? undefined;
  const latest = status?.latest_version ?? undefined;
  const newer = installed && latest ? isNewerVersion(latest, installed) : undefined;
  const method = status?.install_method;
  const sentence = !status
    ? t("settings.accounts.cli.checking")
    : running
      ? t("settings.accounts.cli.updating")
      : !installed
        ? t("settings.accounts.cli.unreadable")
        : latest && newer
          ? t("settings.accounts.cli.available", { version: isolate(installed), latest: isolate(latest) })
          : latest && newer === false
            ? t("settings.accounts.cli.upToDate", { version: isolate(installed) })
            : t("settings.accounts.cli.installed", { version: isolate(installed) });
  // A reason in the sentence alone, without the remedy line of an update that failed, which the error alert holds and
  // the popover beside it does not need; the daemon sends no agent for these codes, so it is supplied here.
  const params = { agent: providerLabel(t, providers, agent) };
  const wording = (code: string) => daemonMessage(language, code, params, code, NO_RECORDS, true);
  // The same sentence with the agent's name as a chip, as the alert draws it, for the popover; `wording` stays the
  // mark's accessible name.
  const richWording = (code: string) => <DaemonErrorMessage error={new CodedError(wording(code), code, params)} part="summary" />;
  const updateError = update?.error ?? "agent_cli_update_failed";
  const updateDetail = update?.detail ?? undefined;
  // What went wrong and blocks nothing, one entry for each: the lookup of the latest version, and the last update.
  const problems = [
    ...(status && !latest && status.latest_error ? [{ code: status.latest_error, detail: status.latest_detail ?? undefined }] : []),
    ...(update?.state === "failed" && newer !== false ? [{ code: updateError, detail: updateDetail }] : []),
  ];
  const canUpdate = !!status && method !== "unknown" && !!installed && newer !== false;
  const ask = () => {
    if (askedFrom !== undefined) return;
    trackAgentCliUpdate(store, agent, update?.started_at ?? null);
    request({ type: "update_agent_cli", agent }).catch((error: unknown) => {
      trackAgentCliUpdate(store, agent, undefined);
      raiseError(error, { title: t("settings.accounts.cli.updateFailedTitle") });
    });
  };
  const announcement = ended === "succeeded"
    ? (installed ? t("settings.accounts.cli.updated", { version: isolate(installed) }) : t("settings.accounts.cli.updatedNoVersion"))
    : ended === "failed" && !seen?.here ? wording(updateError) : undefined;
  return (
    <SettingRow
      compact
      label={t("settings.accounts.cli.label")}
      help={
        <span ref={helpTip} className="contents">
          <HelpTip label={t("settings.accounts.cli.helpLabel")}>{t("settings.accounts.cli.helpText")}</HelpTip>
        </span>
      }
      description={
        <>
          <p>{sentence}</p>
          {method && <p className="text-xs">{t(METHOD_MESSAGES[method])}</p>}
        </>
      }
    >
      <div className="flex items-center gap-2">
        {problems.length > 0 && (
          <>
            {handoffFrom(`[data-status-mark-id="${markId}"]`)}
            {problems.length === 1 ? (
              <StatusMark tone="warning" markId={markId} label={wording(problems[0]!.code)} line={richWording(problems[0]!.code)} detail={problems[0]!.detail} />
            ) : (
              <StatusMark tone="warning" markId={markId} label={problems.map(({ code }) => wording(code)).join(t("common.sentenceSeparator"))}>
                {problems.map(({ code, detail }) => (
                  <div key={code} className="flex flex-col gap-2">
                    <p>{richWording(code)}</p>
                    {detail && <DiagnosticBlock>{detail}</DiagnosticBlock>}
                  </div>
                ))}
              </StatusMark>
            )}
          </>
        )}
        {(canUpdate || running) && (
          <>
            {handoffFrom(`[data-cli-update="${updateId}"]`)}
            <Button data-cli-update={updateId} size="sm" variant={hasCliUpdate(status) ? "primary" : "secondary"} pending={running} onPress={ask}>
              {t("settings.accounts.cli.update")}
            </Button>
          </>
        )}
        <StatusAnnouncer text={announcement} />
      </div>
    </SettingRow>
  );
}
