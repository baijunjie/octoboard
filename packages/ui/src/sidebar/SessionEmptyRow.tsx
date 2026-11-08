import { Bot, BotOff, MessageSquarePlus } from "lucide-react";
import React from "react";

import type { NoAgentReason } from "../agents/agents";
import { EmptyPanel } from "../components/panel/EmptyPanel";
import { PATH_VAR_PARAMS } from "../components/text/literalParams";
import { Message, useT } from "../i18n/react";

export function SessionEmptyRow({ noAgents, message, action }: {
  noAgents?: NoAgentReason;
  message: string;
  action: { label: string; onPress: () => void };
}): React.ReactElement {
  const t = useT();
  if (noAgents) {
    return <EmptyPanel compact icon={noAgents === "add" ? Bot : BotOff} message={noAgents === "add" ? t("agents.noneAdded") : <Message id="agents.noneFound" params={PATH_VAR_PARAMS} />} />;
  }
  return <EmptyPanel compact icon={MessageSquarePlus} message={message} action={action} />;
}
