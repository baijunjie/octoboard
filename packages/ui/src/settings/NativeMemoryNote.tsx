import React from "react";

import { listedAgents, servedAgents } from "../agents/agents";
import { ProviderName } from "../components/text/ProviderName";
import { joinNodes } from "../i18n/joinPhrases";
import { Message, useCurrentLanguage } from "../i18n/react";
import type { Agent, ProviderInfo } from "../protocol";
import { useDaemonStore, useProviders } from "../store";

/** The agents Settings lists (`listedAgents`: the added ones in their order) that keep a memory of their own
 * (`ProviderInfo.native_memory`): those whose memory a session launched while Octoboard's memory is on turns off, and
 * those whose memory such a session leaves as it is set, since no launch can turn it off. An agent the user has not
 * added, one with no memory of its own, and one with a value this build does not know are in neither. */
function nativeMemoryAgents(providers: readonly ProviderInfo[], added: Agent[] | null): { turnedOff: string[]; kept: string[] } {
  const listed = listedAgents(added, servedAgents(providers)).flatMap((id) => providers.filter((provider) => provider.id === id));
  const ids = (memory: ProviderInfo["native_memory"]) => listed.filter((provider) => provider.native_memory === memory).map(({ id }) => id);
  return { turnedOff: ids("switchable"), kept: ids("unswitchable") };
}

/** What the memory switch's description adds about the agents' own memory, a sentence to a line under it: the agents
 * whose own memory those sessions turn off, and the agents whose own memory they leave as it is set. A sentence with no
 * agent to name is left out, so with neither kind listed it is nothing. */
export function NativeMemoryNote(): React.ReactElement | null {
  const language = useCurrentLanguage();
  const added = useDaemonStore((s) => s.settings.added_agents);
  const { turnedOff, kept } = nativeMemoryAgents(useProviders(), added);
  if (turnedOff.length === 0 && kept.length === 0) return null;
  const agents = (ids: string[]) => joinNodes(language, ids, (id, key) => <ProviderName key={key} agent={id} />, "long");
  return (
    <>
      {turnedOff.length > 0 && (
        <span className="block">
          <Message id="settings.memory.enabled.nativeMemory.turnedOff" params={{ agents: agents(turnedOff) }} />
        </span>
      )}
      {kept.length > 0 && (
        <span className="block">
          <Message id="settings.memory.enabled.nativeMemory.kept" params={{ agents: agents(kept) }} />
        </span>
      )}
    </>
  );
}
