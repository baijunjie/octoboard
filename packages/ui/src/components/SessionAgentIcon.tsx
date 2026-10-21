import React from "react";

import { AGENT_LABEL } from "../agents";
import { useT } from "../i18n/react";
import type { Account, Session, Settings } from "../protocol";
import { sessionAccount } from "../sessionLabel";
import { AgentIcon } from "./AgentIcon";
import { InlineChip } from "./InlineChip";
import { StatusBadge } from "./StatusBadge";

/** The tooltip of a session's agent mark: the agent's name, then the account it runs under as a
 * badge. The row wraps and the badge breaks inside a long name or a recorded path, so neither runs
 * past the tooltip's width. The account takes the direction of its own text: a name the user typed
 * is `auto`, a recorded directory `ltr`. */
export function SessionAgentTooltip({ session, accounts, defaultNames }: {
  session: Session;
  accounts: Account[];
  defaultNames: Settings["default_account_names"];
}): React.ReactElement {
  const t = useT();
  const account = sessionAccount(t, session, accounts, defaultNames);
  return (
    <span className="flex flex-wrap items-center gap-x-1.5 gap-y-1">
      <InlineChip size="inherit" dir="auto" wrap>{AGENT_LABEL[session.agent]}</InlineChip>
      {account !== undefined && <StatusBadge overflow="wrap" dir={account.isPath ? "ltr" : "auto"}>{account.isPath ? <code dir="ltr">{account.name}</code> : account.name}</StatusBadge>}
    </span>
  );
}

/** A session's agent mark with the tooltip that names the account it runs under. It lives apart from
 * `AgentIcon` so that one stays free of the i18n state (the native menus build their icons in a
 * node test). */
export function SessionAgentIcon({ session, accounts, defaultNames }: {
  session: Session;
  accounts: Account[];
  defaultNames: Settings["default_account_names"];
}): React.ReactElement {
  return <AgentIcon agent={session.agent} tooltip={<SessionAgentTooltip session={session} accounts={accounts} defaultNames={defaultNames} />} />;
}
