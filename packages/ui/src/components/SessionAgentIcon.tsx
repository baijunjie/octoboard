import React from "react";

import { AGENT_LABEL } from "../agents";
import { useT } from "../i18n/react";
import type { Account, Launch, Session, Settings } from "../protocol";
import { sessionAccount } from "../sessionLabel";
import { sessionModelValues } from "../sessionModel";
import { AgentIcon } from "./AgentIcon";
import { StatusBadge } from "./StatusBadge";

/** The tooltip of a session's agent mark: the agent's name as plain text, then the account it runs
 * under as a badge. Given `launch`, the account, model and effort are a labelled list below the name
 * instead, where a value alone would not say what it is, set off from the name by a rule as the
 * context size's tooltip is. A long name or a recorded path wraps, so nothing runs past the
 * tooltip's width. The account takes the direction of its own text: a name
 * the user typed is `auto`, a recorded directory `ltr`. */
export function SessionAgentTooltip({ session, accounts, defaultNames, launch }: {
  session: Session;
  accounts: Account[];
  defaultNames: Settings["default_account_names"];
  launch?: Launch;
}): React.ReactElement {
  const t = useT();
  const account = sessionAccount(t, session, accounts, defaultNames);
  const accountName = account && (account.isPath ? <code dir="ltr">{account.name}</code> : account.name);
  if (launch) {
    const { model, effort } = sessionModelValues(t, session.agent, launch);
    const rows: { label: string; value: React.ReactNode; dir: "ltr" | "auto" }[] = [
      { label: t("session.agentTooltip.model"), value: model, dir: "auto" },
      { label: t("session.agentTooltip.effort"), value: effort, dir: "auto" },
    ];
    if (account) rows.unshift({ label: t("session.agentTooltip.account"), value: accountName, dir: account.isPath ? "ltr" : "auto" });
    return (
      <div className="flex flex-col gap-1.5">
        <div dir="auto" className="font-medium">{AGENT_LABEL[session.agent]}</div>
        <hr className="groove-t" />
        <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-0.5">
          {rows.map(({ label, value, dir }) => (
            <React.Fragment key={label}>
              <dt className="text-app-muted">{label}</dt>
              <dd dir={dir} className="min-w-0 [overflow-wrap:anywhere]">{value}</dd>
            </React.Fragment>
          ))}
        </dl>
      </div>
    );
  }
  return (
    <span className="flex flex-wrap items-center gap-x-1.5 gap-y-1">
      <span dir="auto">{AGENT_LABEL[session.agent]}</span>
      {account !== undefined && <StatusBadge overflow="wrap" dir={account.isPath ? "ltr" : "auto"}>{accountName}</StatusBadge>}
    </span>
  );
}

/** A session's agent mark with the tooltip `SessionAgentTooltip` draws. It lives apart from `AgentIcon` so that one
 * stays free of the i18n state (the native menus build their icons in a node test). */
export function SessionAgentIcon(props: {
  session: Session;
  accounts: Account[];
  defaultNames: Settings["default_account_names"];
  launch?: Launch;
  /** Whether a tap opens the tooltip's content in a popover; off inside a pressable row, whose tap it is. */
  touchPopover?: boolean;
}): React.ReactElement {
  const { touchPopover, ...tooltip } = props;
  return <AgentIcon agent={props.session.agent} tooltip={<SessionAgentTooltip {...tooltip} />} touchPopover={touchPopover} />;
}
