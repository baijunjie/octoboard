import React, { useRef, useState } from "react";

import { useDismissOnOutsideOrEscape } from "../hooks/useDismissOnOutsideOrEscape";

export interface DropdownOption<T extends string> {
  value: T;
  label: string;
}

interface DropdownProps<T extends string> {
  options: DropdownOption<T>[];
  value: T;
  onChange: (value: T) => void;
  disabled?: boolean;
  "aria-label"?: string;
}

/**
 * A `<select>`-equivalent built from a button and a popover list instead of the native element.
 * Native popup menus on macOS are not reachable through the accessibility tree at all, which rules
 * out scripted UI verification for anything the user must pick from one — this is used everywhere
 * the app would otherwise reach for `<select>`.
 */
export function Dropdown<T extends string>({
  options,
  value,
  onChange,
  disabled,
  "aria-label": ariaLabel,
}: DropdownProps<T>): React.ReactElement {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useDismissOnOutsideOrEscape(open, rootRef, () => setOpen(false));

  const selected = options.find((o) => o.value === value);

  return (
    <div className={`dropdown${open ? " dropdown-open" : ""}`} ref={rootRef}>
      <button
        type="button"
        className="dropdown-trigger"
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={ariaLabel}
        onClick={() => setOpen((o) => !o)}
      >
        <span>{selected?.label ?? value}</span>
        <span className="dropdown-caret" aria-hidden="true" />
      </button>
      {open && (
        <ul className="dropdown-list" role="listbox">
          {options.map((option) => (
            <li key={option.value}>
              <button
                type="button"
                role="option"
                aria-selected={option.value === value}
                className={`dropdown-option${option.value === value ? " dropdown-option-selected" : ""}`}
                onClick={() => {
                  onChange(option.value);
                  setOpen(false);
                }}
              >
                {option.label}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
