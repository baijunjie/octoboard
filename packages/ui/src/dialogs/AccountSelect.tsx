import { Header, Label, ListBox, Select } from "@heroui/react";
import React from "react";

import { choiceKey, findEntry, type AccountChoice, type ChoiceGroup } from "../accountChoices";
import { AGENT_LABEL } from "../agents";
import { AgentAccountText } from "../components/AgentAccountText";
import { AgentIcon } from "../components/AgentIcon";
import { FadeOverflow } from "../components/FadeOverflow";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";

/** One drop-down that settles an agent and one of its accounts together: a group per agent, headed
 * by its icon and name, whose entries are that agent's accounts. An entry names the account alone,
 * the agent being its group's heading; the trigger names both. A group whose agent may not be
 * picked is shown, headed as not installed, with every entry unselectable. It is a control for
 * opening a session, so it offers no way to move an existing one. */
export function AccountSelect({
  label,
  groups,
  value,
  onChange,
}: {
  label: string;
  groups: ChoiceGroup[];
  value: AccountChoice;
  onChange: (choice: AccountChoice) => void;
}): React.ReactElement {
  const t = useT();
  const current = findEntry(groups, choiceKey(value));
  return (
    // HeroUI's variant for a field on a surface: this sits in a dialog.
    <Select
      variant="secondary"
      fullWidth
      value={choiceKey(value)}
      onChange={(key) => {
        const entry = key === null ? undefined : findEntry(groups, String(key));
        if (entry) onChange({ agent: entry.agent, account: entry.account });
      }}
      // Keeps a disabled entry focusable and announced in arrow-key traversal while still refusing
      // the choice; see `OptionSelect` for why it sits on the `Select` and needs the cast.
      {...({ disabledBehavior: "selection" } as React.ComponentProps<typeof Select>)}
    >
      <Label>{label}</Label>
      <Select.Trigger data-marquee-scope>
        <Select.Value className="flex min-w-0 flex-1 items-center gap-2 text-clip">
          {() =>
            current && (
              <>
                <AgentIcon agent={current.agent} />
                <FadeOverflow as="span" className="min-w-0 flex-1" titleWhenClipped={t("session.agentAccount", { agent: AGENT_LABEL[current.agent], account: isolate(current.name) })}>
                  <AgentAccountText agent={AGENT_LABEL[current.agent]} account={current.name} />
                </FadeOverflow>
              </>
            )
          }
        </Select.Value>
        <Select.Indicator />
      </Select.Trigger>
      <Select.Popover className="w-(--trigger-width) max-w-[calc(100vw-2rem)]">
        <ListBox>
          {groups.map((group) => (
            <ListBox.Section key={group.agent}>
              <Header data-marquee-scope className="flex min-w-0 items-center gap-2">
                <AgentIcon agent={group.agent} className="size-3.5" />
                <FadeOverflow className="min-w-0 flex-1" titleWhenClipped={group.selectable ? AGENT_LABEL[group.agent] : t("agents.notInstalled", { agent: AGENT_LABEL[group.agent] })}>
                  {group.selectable ? AGENT_LABEL[group.agent] : t("agents.notInstalled", { agent: AGENT_LABEL[group.agent] })}
                </FadeOverflow>
              </Header>
              {group.entries.map((entry) => (
                <ListBox.Item data-marquee-scope className="pe-7" key={entry.key} id={entry.key} textValue={entry.name} isDisabled={!group.selectable}>
                  <FadeOverflow dir="auto" className="min-w-0 flex-1" titleWhenClipped={entry.name}>{entry.name}</FadeOverflow>
                  <ListBox.ItemIndicator />
                </ListBox.Item>
              ))}
            </ListBox.Section>
          ))}
        </ListBox>
      </Select.Popover>
    </Select>
  );
}
