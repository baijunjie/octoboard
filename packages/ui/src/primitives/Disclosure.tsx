import { Disclosure as HeroDisclosure } from "@heroui/react";
import React from "react";

import { useDisabled } from "./busy";

/** HeroUI's disclosure: a `DisclosureTrigger` that shows and hides its `DisclosureContent`. Expanded while `expanded`,
 * told of presses by `onExpandedChange`; left to itself, collapsed to begin with, without them. */
export function Disclosure({ expanded, onExpandedChange, children }: {
  expanded?: boolean;
  onExpandedChange?: (expanded: boolean) => void;
  children: React.ReactNode;
}): React.ReactElement {
  return <HeroDisclosure isDisabled={useDisabled()} isExpanded={expanded} onExpandedChange={onExpandedChange}>{children}</HeroDisclosure>;
}

/** The heading a `DisclosureTrigger` sits in, for a disclosure that opens a section of the outline. */
export function DisclosureHeading({ children }: { children: React.ReactNode }): React.ReactElement {
  return <HeroDisclosure.Heading>{children}</HeroDisclosure.Heading>;
}

/** The button that shows and hides the disclosure's content; it carries the expanded state. */
export function DisclosureTrigger({ describedBy, className, children }: {
  describedBy?: string;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return <HeroDisclosure.Trigger aria-describedby={describedBy} className={className}>{children}</HeroDisclosure.Trigger>;
}

/** The chevron in a `DisclosureTrigger` that turns with the expanded state. */
export function DisclosureIndicator({ className }: { className?: string }): React.ReactElement {
  return <HeroDisclosure.Indicator className={className} />;
}

/** What the disclosure shows and hides, holding its `DisclosureBody`. */
export function DisclosureContent({ className, children }: { className?: string; children: React.ReactNode }): React.ReactElement {
  return <HeroDisclosure.Content className={className}>{children}</HeroDisclosure.Content>;
}

/** The padded body inside a `DisclosureContent`. */
export function DisclosureBody({ className, children }: { className?: string; children: React.ReactNode }): React.ReactElement {
  return <HeroDisclosure.Body className={className}>{children}</HeroDisclosure.Body>;
}
