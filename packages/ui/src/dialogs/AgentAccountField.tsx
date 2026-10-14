import React from "react";

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
  if (!account) {
    return (
      <OptionSelect
        label={label}
        options={options}
        value={value}
        onChange={onChange}
        isDisabled={isDisabled}
        description={description}
      />
    );
  }
  return (
    <LabelledGroup label={label} description={description} isDisabled={isDisabled}>
      <div className="flex items-stretch">
        <OptionSelect joined="start" label={label} options={options} value={value} onChange={onChange} isDisabled={isDisabled} />
        <OptionSelect
          joined="end"
          label={account.label}
          options={account.options}
          value={account.value}
          onChange={account.onChange}
          isDisabled={isDisabled}
        />
      </div>
    </LabelledGroup>
  );
}
