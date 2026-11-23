import type React from "react";

import { buttonClassName, DisclosureIndicator, DisclosureTrigger } from "../../primitives";

/** The trigger of a disclosure that folds away a form's rarely set fields, drawn as a small ghost button in muted
 * text, by the user's decision, so at rest it reads as an action rather than a field label or a heading; any other
 * disclosure is a `TextDisclosure`. `ms-0` drops the button's negative icon margin on the label's side of the chevron.
 * `className` adds the caller's layout. Used by a dialog's Advanced section (`AdvancedSection`) and, by the user's
 * exception, the welcome screen's tools not detected. */
export function QuietDisclosureTrigger({ className = "", children }: { className?: string; children: React.ReactNode }): React.ReactElement {
  return (
    <DisclosureTrigger className={`${buttonClassName({ variant: "ghost", size: "sm" })} text-app-muted hover:text-foreground ui-keyboard-focus:text-foreground ${className}`}>
      {children}
      <DisclosureIndicator className="ms-0" />
    </DisclosureTrigger>
  );
}
