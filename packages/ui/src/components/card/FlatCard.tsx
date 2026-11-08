import React, { createContext, useContext, useId } from "react";

/** The id of the heading a `FlatCard` is labelled by, for `FlatCard.Heading` to carry. */
const HeadingIdContext = createContext<string | undefined>(undefined);

/** Whether `Rows` sits among the blocks of `FlatCard.Blocks`, which inset it themselves. */
const InBlocksContext = createContext(false);

/**
 * A flat card: a section on a surface that has no other surface to stand off from (a dialog's own background), framed
 * as a table. The rules live here, so a caller writes no divider, inset or fill class of its own:
 *
 * - The card is a `panel-edge` hairline outline, rounded, with no shadow, and its content lies on the surface under it.
 *   It is labelled by its `FlatCard.Heading`.
 * - `FlatCard.Header` is the card's header row on the `--control-fill` fill. It has no line at its edges: where it
 *   meets the outline, the outline is its only edge. It rounds its bottom corners too when it is the card's last child
 *   (a collapsed card).
 * - `FlatCard.Band` is a heading band inside the card body (an agent card's Accounts band): the same fill across the
 *   card's width, no line at its edges, quiet text.
 * - `FlatCard.Rows` (a list) and `FlatCard.Blocks` (a stack of blocks) divide their items by a plain hairline
 *   (`hairline-b`, never a groove) inset by the card's padding (`mx-4`); none follows the last item, so none lies
 *   against a band or the card's edge, and none is drawn against the header, since it is drawn below an item only.
 *
 * Only the header and the band run the card's full width. See "Hand-built rows and cards" and "Where a groove goes" in
 * docs/ui-standard.md.
 */
function FlatCardRoot({ children }: { children: React.ReactNode }): React.ReactElement {
  const headingId = useId();
  return (
    <HeadingIdContext.Provider value={headingId}>
      <section aria-labelledby={headingId} className="flex flex-col rounded-xl panel-edge">
        {children}
      </section>
    </HeadingIdContext.Provider>
  );
}

/** The card's header row. It holds the card's `Heading` (the heading itself, or a title with marks beside it, or a
 * checkbox, which keeps the default variant there: the secondary variant's box takes this same fill and would vanish on
 * it). `className` lays the content out in the row. */
function Header({ className, children }: { className?: string; children: React.ReactNode }): React.ReactElement {
  return <div className={`rounded-t-xl bg-(--control-fill) px-4 py-3 last:rounded-b-xl${className ? ` ${className}` : ""}`}>{children}</div>;
}

/** The card's heading, the element that names the card. `className` sets what the caller needs of it (its own text
 * handling); it has the size and weight of a card title already. */
function Heading({ className, dir, children }: { className?: string; dir?: "auto"; children: React.ReactNode }): React.ReactElement {
  const id = useContext(HeadingIdContext);
  return <h3 id={id} dir={dir} className={`text-base font-semibold text-foreground${className ? ` ${className}` : ""}`}>{children}</h3>;
}

/** A heading band within the card body, over what it introduces: the header's fill across the card's width. */
function Band({ children }: { children: React.ReactNode }): React.ReactElement {
  return <h4 className="bg-(--control-fill) px-4 py-2 text-sm font-medium text-app-muted">{children}</h4>;
}

/** A list of rows, each an `li` divided from the next by the inset hairline. `padded` adds the list's own vertical
 * padding, for rows that take none of their own. */
function Rows({ padded, children }: { padded?: boolean; children: React.ReactNode }): React.ReactElement {
  const inBlocks = useContext(InBlocksContext);
  const dividers = "[&>li]:hairline-b [&>li:last-child]:border-b-0";
  return <ul className={`flex flex-col ${dividers}${inBlocks ? "" : " [&>li]:mx-4"}${padded ? " py-1" : ""}`}>{children}</ul>;
}

/** A stack of blocks (each a direct child: a `div`, or a `Rows` list), divided from the next by the inset hairline. */
function Blocks({ children }: { children: React.ReactNode }): React.ReactElement {
  return (
    <InBlocksContext.Provider value={true}>
      <div className="flex flex-col [&>*]:mx-4 [&>*]:hairline-b [&>*:last-child]:border-b-0">{children}</div>
    </InBlocksContext.Provider>
  );
}

export const FlatCard = Object.assign(FlatCardRoot, { Header, Heading, Band, Rows, Blocks });
