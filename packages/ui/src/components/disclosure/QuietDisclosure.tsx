import type React from "react";

import { buttonClassName, DisclosureBody, DisclosureContent, DisclosureIndicator, DisclosureTrigger } from "../../primitives";

/** The trigger of a disclosure that folds away something rarely needed, drawn as a small ghost button in muted text,
 * by the user's decision, so at rest it reads as an action rather than a field label or a heading. `ms-0` drops the
 * button's negative icon margin on the label's side of the chevron. `className` adds the caller's layout. Used by a
 * dialog's Advanced section (`AdvancedSection`) and the welcome screen's tools not detected. */
export function QuietDisclosureTrigger({ className = "", children }: { className?: string; children: React.ReactNode }): React.ReactElement {
  return (
    <DisclosureTrigger className={`${buttonClassName({ variant: "ghost", size: "sm" })} text-app-muted hover:text-foreground ui-keyboard-focus:text-foreground ${className}`}>
      {children}
      <DisclosureIndicator className="ms-0" />
    </DisclosureTrigger>
  );
}

/** What a disclosure folds away, set flush with what stands above it. HeroUI pads the body by 8px on every side and the
 * content clips what overflows it, so that padding is the room focus rings and shadows inside need. Taking all of it
 * out as a margin would reach past a dialog body, whose own slack is 4px, so the content takes 4px of it back and the
 * body's inner element the other 4px, which lands what is inside in line with what is above it with 4px of slack left
 * inside the clip. The padding above and below is HeroUI's own. `className` goes on the body's inner element. */
export function FlushDisclosureContent({ className = "", children }: { className?: string; children: React.ReactNode }): React.ReactElement {
  return (
    <DisclosureContent className="-mx-1">
      <DisclosureBody className={`-mx-1 ${className}`}>{children}</DisclosureBody>
    </DisclosureContent>
  );
}
