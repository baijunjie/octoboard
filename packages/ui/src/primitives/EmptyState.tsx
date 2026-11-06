import { EmptyState as HeroEmptyState } from "@heroui/react";
import React from "react";

/** HeroUI's empty state: the frame of what stands in for content that is not there. `silent` keeps it from assistive
 * technology, for a state another region already says. */
export function EmptyState({ silent, className, children }: {
  silent?: boolean;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return <HeroEmptyState aria-hidden={silent} className={className}>{children}</HeroEmptyState>;
}
