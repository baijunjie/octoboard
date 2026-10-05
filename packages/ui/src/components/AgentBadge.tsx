import { Chip } from "@heroui/react";
import React from "react";

import { AGENT_LABEL } from "../agents";
import type { Agent } from "../protocol";

/** A small text badge identifying a session's agent — no vendor icon assets are available. */
export function AgentBadge({ agent }: { agent: Agent }): React.ReactElement {
  return (
    <Chip size="sm" variant="soft" className="shrink-0">
      {AGENT_LABEL[agent]}
    </Chip>
  );
}
