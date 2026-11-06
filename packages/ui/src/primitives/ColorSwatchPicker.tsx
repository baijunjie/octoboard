import { ColorSwatchPicker as HeroColorSwatchPicker } from "@heroui/react";
import React from "react";
import { ListBoxContext, parseColor } from "react-aria-components";

import { useDisabled } from "./busy";
import type { DataAttributes } from "./props";

/** HeroUI's colour swatch picker, in its small size: a list of `ColorSwatch`es, one tab stop with the arrow keys
 * moving between them, named by the elements `labelledBy` lists. `value` is the chosen colour as a hex string
 * (`#rrggbb`), or none; `onChange` receives the one chosen, lowercase. With `autoFocus` focus goes into the list as it
 * mounts, onto the chosen swatch. Each swatch holds a real colour, so React Aria names it in the user's language and
 * types ahead on that name. */
export function ColorSwatchPicker({ labelledBy, value, onChange, autoFocus, className, children }: {
  labelledBy?: string;
  value?: string;
  onChange: (value: string) => void;
  autoFocus?: boolean;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    // The picker is a list box, which takes its opening focus from the list box context.
    <ListBoxContext value={{ autoFocus }}>
      <HeroColorSwatchPicker
        aria-labelledby={labelledBy}
        size="sm"
        className={className}
        value={value ? parseColor(value) : undefined}
        onChange={(picked) => onChange(picked.toString("hex").toLowerCase())}
      >
        {children}
      </HeroColorSwatchPicker>
    </ListBoxContext>
  );
}

/** One swatch of a `ColorSwatchPicker`, of `color` (a hex string), with HeroUI's mark while it is the chosen one.
 * `swatchClassName` goes on the colour itself; the `data-*` props on the item around it. */
export function ColorSwatch({ color, swatchClassName, ...data }: DataAttributes & {
  color: string;
  swatchClassName?: string;
}): React.ReactElement {
  return (
    <HeroColorSwatchPicker.Item {...data} color={color} isDisabled={useDisabled()}>
      <HeroColorSwatchPicker.Swatch className={swatchClassName} />
      <HeroColorSwatchPicker.Indicator />
    </HeroColorSwatchPicker.Item>
  );
}
