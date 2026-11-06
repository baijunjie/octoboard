import type React from "react";

import { Disclosure, DisclosureBody, DisclosureContent, DisclosureIndicator, DisclosureTrigger, Surface } from "../../primitives";

/** A disclosure, collapsed until asked, over what an option covers: a real button with a name that
 * stays put, its state carried by `aria-expanded` and the indicator, described by the option it
 * belongs to (`describedBy`). It is no heading, so several in one dialog add nothing to its outline.
 * The expanded content sits on a secondary `Surface`, which sets it apart from the option's own
 * label and hint as one block the option covers. It stays as the user left it for as long as it is
 * shown. */
export function OptionDetails({ describedBy, name, children }: {
  describedBy: string;
  name: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <Disclosure>
      <DisclosureTrigger describedBy={describedBy} className="flex items-center gap-1 text-sm font-medium text-foreground">
        {name}
        <DisclosureIndicator className="ms-0" />
      </DisclosureTrigger>
      <DisclosureContent>
        <DisclosureBody className="px-0">
          <Surface variant="secondary" className="rounded-lg px-3 py-2">
            {children}
          </Surface>
        </DisclosureBody>
      </DisclosureContent>
    </Disclosure>
  );
}
