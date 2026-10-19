import React from "react";

import { agentPickerOptions, agentSetupOptions } from "../agents";
import type { Translate } from "../i18n/catalog";
import type { Agent, AgentAvailability } from "../protocol";
import { AGENT_MARKS } from "./agentMarks";
import { IndicatorTooltip } from "./IndicatorTooltip";

/** The mark of a session's agent. Always decorative: wherever it appears, the agent's name is in
 * the accessible name of the row or written out beside it (`sessionAriaLabel`). `tooltip`, where
 * given, shows on pointer hover only, through `IndicatorTooltip`, without a name of its own, the
 * mark being decorative. */
export function AgentIcon({ agent, className = "size-4", tooltip }: { agent: Agent; className?: string; tooltip?: string }): React.ReactElement {
  const { d, fill } = AGENT_MARKS[agent];
  const icon = (
    <svg viewBox="0 0 24 24" className={`shrink-0 ${className}`} fill={fill} fillRule="evenodd" aria-hidden="true">
      <path d={d} />
    </svg>
  );
  if (!tooltip) return icon;
  return (
    <IndicatorTooltip tooltip={tooltip} className="flex shrink-0">
      {icon}
    </IndicatorTooltip>
  );
}

/** `agentSetupOptions`, with each agent's mark. */
export function agentIconSetupOptions(t: Translate, agentAvailability: Map<Agent, AgentAvailability>, listed: Agent[]) {
  return agentSetupOptions(t, agentAvailability, listed).map((option) => ({ ...option, icon: <AgentIcon agent={option.value} /> }));
}

/** `agentPickerOptions`, with each agent's mark. */
export function agentIconPickerOptions(t: Translate, agentAvailability: Map<Agent, AgentAvailability>, added: Agent[] | null, held?: Agent | null) {
  return agentPickerOptions(t, agentAvailability, added, held).map((option) => ({ ...option, icon: <AgentIcon agent={option.value} /> }));
}
