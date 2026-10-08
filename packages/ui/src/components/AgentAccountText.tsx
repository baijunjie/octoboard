import React from "react";

import { Message } from "../i18n/react";

/** An agent's name followed by the account it runs under, as one sentence-shaped message so the
 * translation decides how they join. The account is a name the user typed, or a path, so it is
 * isolated from the surrounding text's direction. */
export function AgentAccountText({ agent, account }: { agent: string; account?: string }): React.ReactElement {
  if (account === undefined) return <>{agent}</>;
  return <Message id="session.agentAccount" params={{ agent, account: <bdi dir="auto">{account}</bdi> }} />;
}
