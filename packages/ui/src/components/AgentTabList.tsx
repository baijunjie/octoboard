import { Tabs } from "@heroui/react";
import React from "react";

import { AGENT_LABEL } from "../agents";
import type { Agent } from "../protocol";
import { AgentIcon } from "./AgentIcon";
import { useTabScrollLabels } from "./tabScrollLabels";

/** The underline strip of agent tabs, one per agent in `agents`, for the `Tabs` it sits in: each tab
 * the agent's icon and name, and the strip scrolls with its edge labels when the tabs do not fit.
 * Its container takes `className` and the other props of `Tabs.ListContainer`. `tabRef` gives a
 * tab's element a ref callback, for the one a caller means to focus. */
export function AgentTabList({
  agents,
  label,
  tabRef,
  ...container
}: {
  agents: Agent[];
  /** The tab list's accessible name. */
  label: string;
  tabRef?: (agent: Agent) => React.Ref<HTMLDivElement> | undefined;
} & Omit<React.HTMLAttributes<HTMLDivElement>, "children">): React.ReactElement {
  const tabScrollLabels = useTabScrollLabels();
  return (
    <Tabs.ListContainer {...container} render={(props) => <div {...props}>{tabScrollLabels(props.children)}</div>}>
      <Tabs.List aria-label={label} className="p-1.5">
        {agents.map((agent) => (
          <Tabs.Tab key={agent} id={agent} ref={tabRef?.(agent)} className="scroll-mx-1.5 gap-2 whitespace-nowrap hover:text-foreground hover:opacity-100">
            <AgentIcon agent={agent} />
            {AGENT_LABEL[agent]}
            <Tabs.Indicator className="-bottom-1.5" />
          </Tabs.Tab>
        ))}
      </Tabs.List>
    </Tabs.ListContainer>
  );
}
