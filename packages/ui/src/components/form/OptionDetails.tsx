import type React from "react";

import { TextDisclosure } from "../disclosure/TextDisclosure";

/** A disclosure, collapsed until asked, over what an option covers: a `TextDisclosure` whose trigger is a real button
 * with a name that stays put, its state carried by `aria-expanded` and the triangle, described by the option it
 * belongs to (`describedBy`). It is no heading, so several in one dialog add nothing to its outline. The expanded
 * content starts under the trigger's label, indented past the triangle. It stays as the user left it for as long as
 * it is shown. */
export function OptionDetails({ describedBy, name, children }: {
  describedBy: string;
  name: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <TextDisclosure describedBy={describedBy} label={name}>
      {children}
    </TextDisclosure>
  );
}
