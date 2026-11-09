import React from "react";

import { OptionSelect } from "./OptionSelect";

/** A tri-state switch whose follow option carries the inherited value in its badge. */
export function FollowSwitchSelect({ label, labelledBy, describedBy, value, inherit, followLabel, onLabel, offLabel, onChange, isDisabled }: {
  label: string;
  labelledBy?: string;
  describedBy?: string;
  value: boolean | null;
  inherit: boolean;
  followLabel: string;
  onLabel: string;
  offLabel: string;
  onChange: (value: boolean | null) => void;
  isDisabled?: boolean;
}): React.ReactElement {
  return <OptionSelect hideLabel label={label} labelledBy={labelledBy} describedBy={describedBy}
    options={[
      { value: "follow", label: followLabel, badge: inherit ? onLabel : offLabel, badgeInLabel: true },
      { value: "on", label: onLabel },
      { value: "off", label: offLabel },
    ]}
    value={value === null ? "follow" : value ? "on" : "off"}
    onChange={(picked) => onChange(picked === "follow" ? null : picked === "on")} isDisabled={isDisabled} />;
}
