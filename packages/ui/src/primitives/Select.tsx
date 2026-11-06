import { Select as HeroSelect } from "@heroui/react";
import React from "react";

import { useDisabled } from "./busy";
import { FieldClearContext, type FieldVariant, OutsideFieldParts } from "./Field";
import { type DataAttributes, definedProps } from "./props";

/** HeroUI's select: the field around a `Label`, a `SelectTrigger`, a `SelectPopover` holding a `ListBox` of the
 * options (keyed by their ids), a `Description` and a `FieldError`. `value` is the id of the chosen option, or `null`
 * for none, when `placeholder` shows instead; `onChange` receives the one chosen, or `null` when it is cleared.
 *
 * Two treatments hold for every select. A disabled option stays reachable by the arrow keys and announced, though it
 * cannot be chosen, so the reason it cannot be picked is not hidden from anyone not reading the list by mouse: react-
 * aria's default skips it. And, as `TextField`, an invalid select never hands a submit to the browser's constraint
 * validation: whether a form goes ahead is its own decision. */
export function Select({ value, onChange, placeholder, label, labelledBy, describedBy, disabled, invalid, variant, fullWidth, className, children }: {
  value: string | null;
  onChange: (value: string | null) => void;
  placeholder?: string;
  /** The accessible name, for a select with no visible `Label`. */
  label?: string;
  /** The ids of the elements that name it, in place of `label`. */
  labelledBy?: string;
  describedBy?: string;
  disabled?: boolean;
  invalid?: boolean;
  variant?: FieldVariant;
  fullWidth?: boolean;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <HeroSelect
      value={value}
      onChange={(key) => onChange(key === null ? null : String(key))}
      placeholder={placeholder}
      aria-label={label}
      aria-labelledby={labelledBy}
      aria-describedby={describedBy}
      isDisabled={useDisabled(disabled)}
      isInvalid={invalid}
      validationBehavior="aria"
      variant={variant}
      fullWidth={fullWidth}
      className={className}
      // `disabledBehavior` belongs on the select, not its list box: the list renders the state the select builds
      // (`useSelectState`) and ignores its own. Neither HeroUI's nor react-aria's select props declare it, though
      // `useSelectState` passes its props through to react-stately's `useListState`, which reads it.
      {...({ disabledBehavior: "selection" } as object)}
    >
      {children}
    </HeroSelect>
  );
}

/** The button of a `Select` that opens its options, showing `value` (what the caller draws for the chosen option, or
 * for the placeholder) in a box classed `valueClassName`, then `children` (room kept for what is laid over the
 * trigger), then HeroUI's chevron. */
export function SelectTrigger({ ref, value, valueClassName, className, children, ...data }: DataAttributes & {
  ref?: React.Ref<HTMLButtonElement>;
  value: React.ReactNode;
  valueClassName?: string;
  className?: string;
  children?: React.ReactNode;
}): React.ReactElement {
  return (
    <HeroSelect.Trigger {...data} ref={ref} {...definedProps({ className })}>
      <HeroSelect.Value className={valueClassName}>{() => value}</HeroSelect.Value>
      {children}
      <HeroSelect.Indicator />
    </HeroSelect.Trigger>
  );
}

/** The popover of a `Select` that holds its `ListBox`, under the trigger; `bottom start` aligns it with the trigger's
 * start edge, for a popover wider than the trigger. */
export function SelectPopover({ placement, className, children }: {
  placement?: "bottom start";
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return <HeroSelect.Popover {...definedProps({ placement, className })}>{children}</HeroSelect.Popover>;
}

/** The clear button of a `Select` that offers no choice (`placeholder`): `children`, a layer `Button` placed beside the
 * trigger (a button cannot sit inside another), becomes HeroUI's clear action, which sets the value to `null`, and the
 * trigger then clears on Backspace or Delete too. `onClear` follows a press that cleared. The button carries
 * `data-empty` while there is nothing to clear. */
export function SelectClearButton({ onClear, children }: { onClear?: () => void; children: React.ReactElement }): React.ReactElement {
  return (
    <OutsideFieldParts>
      <HeroSelect.ClearButton<"button">
        {...definedProps({ onClick: onClear && (() => onClear()) })}
        render={({ className: _className, "aria-hidden": _hidden, children: _children, ...clear }) => (
          <FieldClearContext.Provider value={clear}>{children}</FieldClearContext.Provider>
        )}
      />
    </OutsideFieldParts>
  );
}
