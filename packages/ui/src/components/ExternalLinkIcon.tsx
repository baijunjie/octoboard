import { SquareArrowOutUpRight } from "lucide-react";
import type { ReactElement } from "react";

/** The same outbound marker also fits native menu items that cannot contain an anchor. */
export function ExternalLinkIcon({ className = "size-3.5" }: { className?: string }): ReactElement {
  return <SquareArrowOutUpRight aria-hidden="true" className={`inline-block shrink-0 align-[-0.125em] ${className}`} />;
}
