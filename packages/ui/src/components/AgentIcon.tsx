import React from "react";

import { AGENT_OPTIONS, agentPickerOptions } from "../agents";
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

/** The agents as `OptionSelect` options, each with its mark. */
export const AGENT_ICON_OPTIONS = AGENT_OPTIONS.map((option) => ({ ...option, icon: <AgentIcon agent={option.value} /> }));

/** `agentPickerOptions`, with each agent's mark: an agent the machine does not have is named
 * rather than hidden, and cannot be selected. */
export function agentIconPickerOptions(t: Translate, agentAvailability: Map<Agent, AgentAvailability>) {
  return agentPickerOptions(t, agentAvailability).map((option) => ({ ...option, icon: <AgentIcon agent={option.value} /> }));
}
