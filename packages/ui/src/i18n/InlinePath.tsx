import React from "react";
import { ChevronRight } from "lucide-react";

import { InlineReference, type InlineReferenceProps } from "./InlineReference";

export type InlinePathProps = Pick<InlineReferenceProps, "size" | "color"> & {
  /** The names to go through, in order: each a name already in the user's language. */
  steps: readonly string[];
};

/** The plain-text form of a path, for a surface that cannot show the component (an accessible name, an announcement, a
 * system notification, the text of an error): the steps joined by ` > `. */
export function pathText(steps: readonly string[]): string {
  return steps.join(" > ");
}

/** A navigation path mixed into running copy (General, Sharing, Remote Login): one `InlineReference` per step, a chevron
 * between them. A step moves to the next line whole and is broken inside only when it is longer than the line. It is
 * one isolate that follows the interface's direction, so the steps run right to left in a right-to-left language with
 * the chevrons mirrored, and each name takes its own direction. The chevrons are decoration; the text a reader gets is
 * `pathText`. */
export function InlinePath({ steps, size, color }: InlinePathProps): React.ReactElement {
  return (
    <span data-inline-path="" style={{ unicodeBidi: "isolate" }}>
      {steps.map((step, index) => (
        <React.Fragment key={index}>
          {index > 0 && (
            <>
              <wbr />
              <ChevronRight aria-hidden="true" data-inline-path-separator="" className="mx-px inline size-[1em] align-[-0.125em] text-app-muted rtl:-scale-x-100" />
              <span className="sr-only"> &gt; </span>
            </>
          )}
          <InlineReference dir="auto" wrap unbroken size={size} color={color}>{step}</InlineReference>
        </React.Fragment>
      ))}
    </span>
  );
}
