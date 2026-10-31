import { useState } from "react";

/**
 * What a single-line field shows apart from what its form holds: the form always gets the value
 * with its leading and trailing whitespace removed, while the field keeps what was typed until it
 * loses focus, so a space typed between two words is not stripped on its way to becoming the next
 * word. A `value` set from outside (a directory picked, a reset) replaces what the field shows.
 * Spread the result onto the field: `value`, `onChange` and `onBlur`.
 */
export function useTrimmedField(
  value: string,
  onChange: (value: string) => void,
): { value: string; onChange: (next: string | React.ChangeEvent<HTMLInputElement>) => void; onBlur: () => void } {
  // `held` is the value last handed to the form, so a `value` that differs from it came from
  // outside. Comparing against `shown.trim()` instead would never settle on a value set from
  // outside that itself has leading or trailing whitespace (a name stored before trimming).
  const [field, setField] = useState({ shown: value, held: value });
  let current = field;
  if (value !== field.held) {
    current = { shown: value, held: value };
    setField(current);
  }
  return {
    value: current.shown,
    onChange: (next) => {
      const text = typeof next === "string" ? next : next.target.value;
      setField({ shown: text, held: text.trim() });
      onChange(text.trim());
    },
    onBlur: () => setField({ shown: value, held: value }),
  };
}
