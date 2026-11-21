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

/** What a disclosure folds away, set flush with what stands above it. The content clips what overflows it, so focus
 * rings and shadows inside need room within the clip: the content reaches 4px past each side, which is the slack a
 * dialog body has, and the body pads those 4px back in place of HeroUI's 8px, which lands what is inside in line with
 * what is above it. Below, the body keeps 4px for the same rings; above, HeroUI's 8px, the gap to the trigger, which
 * as a margin would stay when the disclosure is folded. `className` goes on the body's inner element. */
export function FlushDisclosureContent({ className = "", children }: { className?: string; children: React.ReactNode }): React.ReactElement {
  return (
    <DisclosureContent className="-mx-1">
      <DisclosureBody flush className={className}>{children}</DisclosureBody>
    </DisclosureContent>
  );
}
