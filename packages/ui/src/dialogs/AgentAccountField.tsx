import React, { useRef } from "react";

import { useFocusHandoff } from "../components/useFocusHandoff";

import { LabelledGroup } from "./LabelledGroup";
import { OptionSelect, type Option } from "./OptionSelect";

/** An agent picker, joined to an account picker with it when `account` is given. The agent and its
 * account read as one setting, so they are one input group under one label: the agent at the start
 * settles which accounts the part at the end offers. Without `account` — an agent with nothing to
 * choose between — it is the agent picker alone. */
export function AgentAccountField<T extends string>({
  label,
  options,
  value,
  onChange,
  account,
  isDisabled,
  description,
}: {
  label: string;
  options: Option<T>[];
  value: T;
  onChange: (value: T) => void;
  account?: { label: string; options: Option<string>[]; value: string; onChange: (value: string) => void };
  isDisabled?: boolean;
  description?: React.ReactNode;
}): React.ReactElement {
  const controls = useRef<HTMLDivElement>(null);
  const agentTrigger = () => controls.current?.querySelector<HTMLElement>('[data-slot="select-trigger"]');
  const focusHandoff = useFocusHandoff(() => agentTrigger()?.focus());
  return (
    <LabelledGroup label={label} description={account ? description : undefined} isDisabled={isDisabled}>
      <div ref={controls} {...focusHandoff} className="flex items-stretch">
        {/* Keeping the agent picker mounted lets its popover restore focus when the next agent
            has no accounts to choose between. */}
        <OptionSelect joined={account ? "start" : undefined} hideLabel label={label} options={options} value={value} onChange={onChange} isDisabled={isDisabled} description={account ? undefined : description} />
        {account && <OptionSelect
          joined="end"
          label={account.label}
          options={account.options}
          value={account.value}
          onChange={account.onChange}
          isDisabled={isDisabled}
        />}
      </div>
    </LabelledGroup>
  );
}
