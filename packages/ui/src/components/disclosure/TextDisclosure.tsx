import type React from "react";

import { Disclosure, DisclosureHeading, DisclosureTrigger } from "../../primitives";
import { FlushDisclosureContent } from "./FlushDisclosureContent";

/** The marker of a disclosure that is not a form's: a filled triangle in the text's colour, the size of the browser's
 * own `summary` marker, pointing to the inline end, and pointing down while `group/disclosure` (the trigger, or the
 * `details` its summary opens) is expanded. It switches between the two shapes rather than turning one (see "Text
 * direction" in `docs/conventions/ui-accessibility.md`), and mirrors in a right-to-left layout. Drawn by
 * `TextDisclosure`, and by a summary that lays out a row of its own (a list's row). */
export function DisclosureTriangle({ className = "" }: { className?: string }): React.ReactElement {
  return (
    <svg aria-hidden="true" viewBox="0 0 12 12" className={`size-3 shrink-0 fill-current rtl:-scale-x-100 ${className}`}>
      <path d="M2.5 1.5 10 6l-7.5 4.5z" className="group-aria-expanded/disclosure:hidden group-open/disclosure:hidden" />
      <path d="M1.5 2.5h9L6 10z" className="hidden group-aria-expanded/disclosure:block group-open/disclosure:block" />
    </svg>
  );
}

/** Extends a 20px trigger's pointer target to 24px above and below without changing what it draws ("Pointer target
 * size" in `docs/conventions/ui-accessibility.md`); the trigger stands outside the content's clip. */
export const EXTENDED_TARGET = "relative after:absolute after:inset-x-0 after:-inset-y-0.5 after:content-['']";

/** A disclosure outside a form's fields (details of an error, what an option covers, a text someone wrote): its trigger
 * is the label as text with a `DisclosureTriangle` before it, collapsed to begin with. What it folds away either starts
 * under the label, indented past the triangle, or, when it is a code block (a `DiagnosticBlock` or a `CodeBlock`, each
 * possibly under a label of its own), `code` sets it flush with what stands above, as the block's frame already sets it
 * apart. The content keeps the 4px of slack a dialog body has for the focus rings inside (see
 * `FlushDisclosureContent`). `heading` puts the trigger in a heading, for a disclosure that opens a section of the
 * outline; `id` is the trigger's, for a block inside that is labelled by it; `describedBy` describes it.
 * `contentClassName` goes on the body's inner element. A form's rarely set fields fold under a
 * `QuietDisclosureTrigger` instead. */
export function TextDisclosure({
  label,
  code = false,
  heading = false,
  id,
  describedBy,
  expanded,
  onExpandedChange,
  contentClassName,
  children,
}: {
  label: React.ReactNode;
  code?: boolean;
  heading?: boolean;
  id?: string;
  describedBy?: string;
  expanded?: boolean;
  onExpandedChange?: (expanded: boolean) => void;
  contentClassName?: string;
  children: React.ReactNode;
}): React.ReactElement {
  const trigger = (
    <DisclosureTrigger
      id={id}
      describedBy={describedBy}
      className={`group/disclosure inline-flex max-w-full items-center gap-1 rounded-sm text-start text-sm text-foreground ${EXTENDED_TARGET}`}
    >
      <DisclosureTriangle />
      <span className="min-w-0">{label}</span>
    </DisclosureTrigger>
  );
  return (
    <Disclosure expanded={expanded} onExpandedChange={onExpandedChange}>
      {heading ? <DisclosureHeading>{trigger}</DisclosureHeading> : trigger}
      <FlushDisclosureContent indent={!code} className={contentClassName}>{children}</FlushDisclosureContent>
    </Disclosure>
  );
}
