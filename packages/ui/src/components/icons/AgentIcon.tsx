import React from "react";

import { agentPickerOptions, agentSetupOptions } from "../../agents/agents";
import type { Translate } from "../../i18n/catalog";
import type { Agent, AgentAvailability, ProviderInfo } from "../../protocol";
import { markArt } from "../../providers/markArt";
import { providerMark } from "../../providers/registry";
import { Tooltip } from "../../primitives";

/** The mark of a session's agent. Always decorative: wherever it appears, the agent's name is in
 * the accessible name of the row or written out beside it (`sessionAriaLabel`). `tooltip`, where
 * given, shows on pointer hover only, through a mark `Tooltip`, without a name of its own, the
 * mark being decorative. A tap opens the tooltip's content in a popover unless `touchPopover` is off, which
 * a mark inside a pressable row turns off, as the tap is the row's. */
export function AgentIcon({
  agent,
  className = "size-4",
  tooltip,
  touchPopover,
}: {
  agent: Agent;
  className?: string;
  tooltip?: React.ReactNode;
  touchPopover?: boolean;
}): React.ReactElement {
  const mark = providerMark(agent);
  // A safe CSS and SVG id, as `useId`'s colons are not.
  const art = markArt(mark, `mark${React.useId().replace(/[^a-zA-Z0-9]/g, "")}`);
  // The art is the provider module's own constant SVG, never text from the daemon or the user.
  const icon = art ? (
    <svg viewBox="0 0 24 24" className={`shrink-0 ${className}`} fill="none" aria-hidden="true" dangerouslySetInnerHTML={{ __html: art }} />
  ) : (
    <svg viewBox="0 0 24 24" className={`shrink-0 ${className}`} fill={mark.fill} fillRule="evenodd" aria-hidden="true">
      <path d={mark.d} />
    </svg>
  );
  if (!tooltip) return icon;
  return (
    <Tooltip trigger="mark" content={tooltip} className="flex shrink-0" touchPopover={touchPopover}>
      {icon}
    </Tooltip>
  );
}

/** `agentSetupOptions`, with each agent's mark. */
export function agentIconSetupOptions(t: Translate, providers: readonly ProviderInfo[], agentAvailability: Map<Agent, AgentAvailability>, listed: Agent[]) {
  return agentSetupOptions(t, providers, agentAvailability, listed).map((option) => ({ ...option, icon: <AgentIcon agent={option.value} /> }));
}

/** `agentPickerOptions`, with each agent's mark. */
export function agentIconPickerOptions(t: Translate, agentAvailability: Map<Agent, AgentAvailability>, added: Agent[] | null, providers: readonly ProviderInfo[], held?: Agent | null) {
  return agentPickerOptions(t, agentAvailability, added, providers, held).map((option) => ({ ...option, icon: <AgentIcon agent={option.value} /> }));
}
