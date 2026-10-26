import React, { useLayoutEffect, useState } from "react";

import { accountDisabledUnavailable, handoffBlock, incompleteAccount, sharingChangeRuns, switchEntries, switchEntryBlock } from "../accountChoices";
import { inheritedSessionLaunch, launchWithOverrides } from "../agents";
import { ErrorAlert } from "../components/ErrorAlert";
import { PathText } from "../components/PathText";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { StatusMark } from "../components/StatusMark";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import type { Account, Agent, Console, Project, Session } from "../protocol";
import { accountUsage } from "../quota/accountUsage";
import { quotaUsedUpMessage } from "../quota/quotaCopy";
import type { SessionRequest } from "../store";
import { useDaemonStore } from "../store";
import { useNow } from "../useNow";
import { AgentAccountField } from "./AgentAccountField";
import { LaunchSettingsFields } from "./LaunchSettingsFields";
import { OptionSelect, type Option } from "./OptionSelect";
import { useLaunchChoice } from "./useLaunchChoice";

/** What the user changed in the form of a switch or a handoff, as `answer_session_request` carries
 * it: each value absent where the request's own stands, `null` for the agent's own default (and, for
 * `account`, the agent's default account). */
export interface RequestEdits {
  agent?: Agent;
  account?: string | null;
  model?: string | null;
  effort?: string | null;
}

/** What a form of a switch or a handoff tells its dialog: the request it is for, the values it ended
 * on, and whether Confirm has to wait (the account chosen cannot take the session, say). Naming the
 * request keeps one form's report from ever holding another request's dialog. */
export interface RequestFormState {
  requestId: string;
  edits: RequestEdits;
  unavailable: boolean;
}

type SwitchRequest = Extract<SessionRequest, { action: "switch" }>;
type HandoffRequest = Extract<SessionRequest, { action: "handoff" }>;
type OnReport = (state: RequestFormState | undefined) => void;

/** Whether the request is to switch a session's account or hand it off, which the dialog answers
 * with the values of a form. */
export function isChangeRequest(request: SessionRequest): request is SwitchRequest | HandoffRequest {
  return request.action === "switch" || request.action === "handoff";
}

/** The warning mark of an account whose last sharing change did not finish (`incompleteAccount`): a session
 * launched on it may find its configuration and conversations incomplete. Where the dialog is the
 * confirmation, as here, this is all it says; the user's own resume asks first instead
 * (`useConfirmIncompleteLaunch`). `inline` is the form for a mark in a sentence; `markId` scopes a focus
 * hand-off to this mark. */
export function IncompleteSharingMark({ account, inline, markId }: { account: Account; inline?: boolean; markId?: string }): React.ReactElement {
  const t = useT();
  return (
    <StatusMark tone="warning" inline={inline} markId={markId} label={t("dialog.sessionRequest.incomplete.mark")}>
      <p>{t("dialog.sessionRequest.incompleteTitle", { account: isolate(account.name) })}</p>
      <p>{t("dialog.sessionRequest.incomplete")}</p>
    </StatusMark>
  );
}

/** Says once that `account`, which the dialog marks, has a sharing change that did not finish, where the
 * mark appears after the dialog opened (the account's change ended unfinished, or the user chose it).
 * A mark the dialog opened with is part of what it opened with and stays unspoken; a mark that moves
 * to another incomplete account while the announcer stays is a new one and is spoken. Mounted for as
 * long as the request is, whether or not there is a mark, and keyed by the request so that the next
 * request in a queue starts afresh. */
export function IncompleteSharingAnnouncer({ account }: { account: Account | undefined }): React.ReactElement {
  const t = useT();
  const [openedWith] = useState(account?.id);
  return <StatusAnnouncer text={account && account.id !== openedWith ? t("dialog.sessionRequest.incompleteTitle", { account: isolate(account.name) }) : undefined} />;
}

/** The key of the default account among the options of the account picker. */
const DEFAULT_ACCOUNT_KEY = "default";

/** Tells the dialog what the form ended on, and that it is gone once it unmounts. A layout effect,
 * so that Confirm is not enabled for a paint before the form has said it must wait. */
function useReportForm(state: RequestFormState, onReport: OnReport): void {
  const { requestId, unavailable, edits } = state;
  const key = [requestId, unavailable, edits.agent, edits.account, edits.model, edits.effort].map(String).join("\0");
  useLayoutEffect(() => onReport(state), [key]);
  useLayoutEffect(() => () => onReport(undefined), []);
}

/** The form of a request to switch a session's account: the account, starting on the one asked for
 * and never the one the session is on, and the model and reasoning effort, starting on the session's
 * own. The model and effort the user changes become the session's own; those left alone are not
 * sent. The account entries follow the Switch account submenu's rules (`switchEntryBlock`), so an
 * account whose quota is used up or whose sharing is being changed is listed but cannot be chosen,
 * and an account that is turned off is left out unless it is the session's own or the one chosen,
 * when it is listed and cannot be chosen; a requested account that no longer exists is said so.
 * `isDisabled` locks the form while the approval is carried out. */
export function SwitchRequestForm({ request, onReport, isDisabled }: { request: SwitchRequest; onReport: OnReport; isDisabled: boolean }): React.ReactElement | null {
  const t = useT();
  const session = useDaemonStore((s) => s.sessions.get(request.target));
  const parentConsole = useDaemonStore((s) => s.consoles.get(request.console));
  const project = useDaemonStore((s) => (request.project ? s.projects.get(request.project) : undefined));
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  const quotas = useDaemonStore((s) => s.accountQuotas);
  const sharing = useDaemonStore((s) => s.accountSharing);
  const disabledDefaults = useDaemonStore((s) => s.settings.disabled_default_accounts);
  const now = useNow();
  const [account, setAccount] = useState<string | null>(request.account);
  const [changed, setChanged] = useState<{ model?: string | null; effort?: string | null }>({});

  const entries = session ? switchEntries(session, accounts, t, defaultNames, { quotas, now }, sharing, disabledDefaults) : [];
  const ownChanging = entries.some((entry) => entry.current && entry.changingSharing);
  const blockOf = (entry: (typeof entries)[number]) =>
    switchEntryBlock(t, entry, { switching: session?.switching_account === true, ownChanging }) ?? (entry.disabled && !entry.current ? accountDisabledUnavailable(t) : undefined);
  const chosen = entries.find((entry) => entry.account === account);
  const incomplete = incompleteAccount(accounts, sharing, account);
  const missing = session !== undefined && chosen === undefined;
  const chosenBlock = chosen && blockOf(chosen);
  const chosenDisabled = chosen !== undefined && !chosen.current && chosen.disabled;
  // The menu leaves Switch account out for a session that is still starting or archived; an archived one ends the
  // dialog's purpose and is the dialog's own failure (`useSessionRequestDialogProps`).
  const sessionBlock = session?.status === "starting" ? t("dialog.sessionRequest.switch.starting") : undefined;
  const notOffered = session !== undefined && (sessionBlock !== undefined || chosen === undefined || chosen.current || chosen.exhaustedBy !== undefined || chosenBlock !== undefined);
  useReportForm({ requestId: request.requestId, edits: { account, ...changed }, unavailable: notOffered }, onReport);
  if (!session) return null;

  // What is wrong with the account chosen is that field's error. A hold on the session itself (an account switch
  // running, its own account's sharing changing) is not about the field: it stays an Alert.
  const sharingBlock = chosenBlock && chosen?.changingSharing && session.switching_account !== true ? chosenBlock.detail : undefined;
  const accountError = missing
    ? t("dialog.sessionRequest.switch.accountGone")
    : chosen?.current
      ? t("dialog.sessionRequest.switch.alreadyOn", { account: isolate(chosen.name) })
      : chosenDisabled
        ? t("accountChoice.disabled.description", { name: isolate(chosen.name) })
        : chosen && chosen.exhaustedBy && chosen.quota
          ? quotaUsedUpMessage(t, chosen.quota, chosen.name, chosen.exhaustedBy, now)
          : sharingBlock;
  const sessionHold = !chosenDisabled && sharingBlock === undefined ? chosenBlock?.detail : undefined;
  const options = entries.filter((entry) => entry.current || !entry.disabled || entry.account === account).map((entry): Option<string> => {
    const usage = accountUsage(t, session.agent, entry.quota, entry.current ? undefined : entry.exhaustedBy, now);
    const block = blockOf(entry);
    const note = block ?? usage;
    const option: Option<string> = {
      value: entry.account ?? DEFAULT_ACCOUNT_KEY,
      label: entry.name,
      // A removed account is its directory, which reads left to right.
      ...(entry.isPath ? { labelContent: <PathText path={entry.name} as="span" className="block" /> } : {}),
      icon: usage.icon,
      ...(entry.current ? { badge: t("dialog.sessionRequest.switch.current") } : {}),
      isDisabled: entry.current || entry.exhaustedBy !== undefined || block !== undefined,
    };
    return note.detail !== undefined ? { ...option, detail: note.detail, reason: note.reason } : option;
  });
  const inherited = parentConsole ? inheritedSessionLaunch(parentConsole, project, session.agent) : { model: null, effort: null };
  const { model, effort } = launchWithOverrides(launchWithOverrides(inherited, { model: session.model, effort: session.effort }), changed);

  return (
    <>
      <IncompleteSharingAnnouncer account={incomplete} />
      {sessionHold && <ErrorAlert title={sessionHold} announce />}
      {sessionBlock && <ErrorAlert title={sessionBlock} announce />}
      {/* Nothing to show as chosen while the account asked for is gone, so the optional picker then;
          its clear button has nothing to clear. One slot, so the picker keeps its focus once the
          user has picked. */}
      {missing
        ? (
          <OptionSelect
            label={t("dialog.sessionRequest.switch.account")}
            options={options}
            value={null}
            placeholder={t("dialog.sessionRequest.switch.account")}
            onChange={(key: string | null) => key !== null && setAccount(key === DEFAULT_ACCOUNT_KEY ? null : key)}
            isDisabled={isDisabled}
            errorMessage={accountError}
          />
        )
        : (
          <OptionSelect
            label={t("dialog.sessionRequest.switch.account")}
            options={options}
            value={account ?? DEFAULT_ACCOUNT_KEY}
            onChange={(key: string) => setAccount(key === DEFAULT_ACCOUNT_KEY ? null : key)}
            isDisabled={isDisabled}
            errorMessage={accountError}
            mark={incomplete ? (markId) => <IncompleteSharingMark account={incomplete} markId={markId} /> : undefined}
          />
        )}
      <LaunchSettingsFields
        agent={session.agent}
        account={missing ? session.account_id ?? null : account}
        model={model}
        effort={effort}
        onModelChange={(value) => setChanged((current) => ({ ...current, model: value }))}
        onEffortChange={(value) => setChanged((current) => ({ ...current, effort: value }))}
        note={t("dialog.sessionRequest.switch.launchNote")}
        isDisabled={isDisabled}
      />
    </>
  );
}

/** The form of a request to hand a session off: the agent, which is never the session's own, joined
 * with its account, and the model and reasoning effort, all as the user's own Hand off dialog has
 * them, starting on what the call asked for. Nothing the asking session wrote is shown. It cannot
 * be confirmed while the session cannot be handed off, which the Hand off menu item refuses for the
 * same reasons: it is working or starting, locked, held by an account switch, or on an account
 * whose sharing is changing. `isDisabled` locks the form while the approval is carried out. */
export function HandoffRequestForm({ request, onReport, isDisabled }: { request: HandoffRequest; onReport: OnReport; isDisabled: boolean }): React.ReactElement | null {
  const parentConsole = useDaemonStore((s) => s.consoles.get(request.console));
  const project = useDaemonStore((s) => (request.project ? s.projects.get(request.project) : undefined));
  const original = useDaemonStore((s) => s.sessions.get(request.target));
  const ownChanging = useDaemonStore((s) => sharingChangeRuns(s.accountSharing, original?.account_id ?? null));
  if (!parentConsole) return <NoEdits request={request} onReport={onReport} />;
  return <HandoffFields request={request} onReport={onReport} console={parentConsole} project={project} original={original} ownChanging={ownChanging} isDisabled={isDisabled} />;
}

/** The form of a handoff whose console the window does not know, which has nothing to change: the
 * approval carries the request's own values. */
function NoEdits({ request, onReport }: { request: HandoffRequest; onReport: OnReport }): null {
  useReportForm({ requestId: request.requestId, edits: {}, unavailable: false }, onReport);
  return null;
}

function HandoffFields({ request, onReport, console: parentConsole, project, original, ownChanging, isDisabled }: {
  request: HandoffRequest;
  onReport: OnReport;
  console: Console;
  project: Project | undefined;
  original: Pick<Session, "status" | "locked" | "switching_account" | "agent"> | undefined;
  ownChanging: boolean;
  isDisabled: boolean;
}): React.ReactElement {
  const t = useT();
  const { choice, blocked, presetGone, unavailable, overrides, agentOptions, setPickedAgent, account, launch } = useLaunchChoice(parentConsole, project, original?.agent, undefined, {
    choice: { agent: request.agent, account: request.account },
    model: request.model,
    effort: request.effort,
  });
  const sessionBlock = original && handoffBlock(t, original, ownChanging);
  const accounts = useDaemonStore((state) => state.settings.accounts);
  const sharing = useDaemonStore((state) => state.accountSharing);
  const incomplete = presetGone ? undefined : incompleteAccount(accounts, sharing, choice.account);
  useReportForm({
    requestId: request.requestId,
    edits: { agent: choice.agent, account: choice.account, ...overrides },
    unavailable: blocked || unavailable !== undefined || sessionBlock !== undefined || presetGone,
  }, onReport);
  const locked = isDisabled || blocked;

  return (
    <>
      <IncompleteSharingAnnouncer account={incomplete} />
      {sessionBlock && <ErrorAlert title={sessionBlock.detail} announce />}
      <AgentAccountField
        label={t("dialog.handOff.agent")}
        options={agentOptions}
        value={choice.agent}
        onChange={setPickedAgent}
        isDisabled={locked}
        account={account && { ...account, mark: incomplete ? (markId) => <IncompleteSharingMark account={incomplete} markId={markId} /> : undefined }}
        errorMessage={presetGone ? t("dialog.sessionRequest.handoff.presetGone") : unavailable}
        placeholders={presetGone ? { agent: t("dialog.sessionRequest.handoff.agentPlaceholder"), account: t("dialog.sessionRequest.handoff.accountPlaceholder") } : undefined}
      />
      <LaunchSettingsFields
        {...launch}
        note={t(project ? "dialog.handOff.launchInheritanceNote" : "dialog.handOff.consoleLaunchInheritanceNote")}
        isDisabled={locked || presetGone}
      />
    </>
  );
}
