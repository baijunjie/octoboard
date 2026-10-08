import { Header, Label, ListBox, Select } from "@heroui/react";
import React from "react";

import { choiceKey, findEntry, type AccountChoice, type ChoiceGroup } from "../accountChoices";
import { AGENT_LABEL } from "../agents";
import { AgentAccountText } from "../components/AgentAccountText";
import { AgentIcon } from "../components/AgentIcon";
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
    >
      <Label>{label}</Label>
      <Select.Trigger>
        <Select.Value className="flex items-center gap-2">
          {() =>
            current && (
              <>
                <AgentIcon agent={current.agent} />
                <span className="truncate">
                  <AgentAccountText agent={AGENT_LABEL[current.agent]} account={current.name} />
                </span>
              </>
            )
          }
        </Select.Value>
        <Select.Indicator />
      </Select.Trigger>
      <Select.Popover>
        {/* `"selection"` keeps a disabled entry focusable and announced in arrow-key traversal while
            still refusing the choice; see `OptionSelect` for why this needs the cast. */}
        <ListBox {...({ disabledBehavior: "selection" } as React.ComponentProps<typeof ListBox>)}>
          {groups.map((group) => (
            <ListBox.Section key={group.agent}>
              <Header className="flex items-center gap-2">
                <AgentIcon agent={group.agent} className="size-3.5" />
                {group.selectable ? AGENT_LABEL[group.agent] : t("agents.notInstalled", { agent: AGENT_LABEL[group.agent] })}
              </Header>
              {group.entries.map((entry) => (
                <ListBox.Item key={entry.key} id={entry.key} textValue={entry.name} isDisabled={!group.selectable}>
                  <span dir="auto">{entry.name}</span>
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
