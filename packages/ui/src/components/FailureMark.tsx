import { Popover } from "@heroui/react";
import { CircleAlert, TriangleAlert } from "lucide-react";
import React, { useRef, useState } from "react";

import { AnchoredTooltip } from "./AnchoredTooltip";
import { Button } from "./Button";
import { TitledControl } from "./TitledControl";
import { useStableHandler } from "./useLatest";

/**
 * The minimal mark of a failure that belongs to one row or table cell: a status glyph, named by
 * `label`, whose press opens the reason and any details in a popover. No sentence of error text goes
 * into the row itself ("A failure that belongs to one row" in `docs/ui-accessibility.md`). The glyph
 * is a control, so it is a tab stop, and in a tree row also a stop of the row's arrow-key walk
 * (ArrowRight from the row). Its tooltip repeats the name and is held back while the popover is
 * open. `children` is the popover's body, which is itself the details surface, so it is ordered: what
 * happened, the main remedy, any further explanation in the application's own words, then the raw
 * text (`FailureBody`), and `actions` after it; with none, the
 * mark is the glyph alone, named and carrying the name in a hover tooltip. That one is no control, so
 * it adds no stop of either kind to the text or row around it, and it is made of spans to sit inside
 * a paragraph; a sighted keyboard user does not get its tooltip (a kept shortfall of "A tooltip on
 * a mark that is not a control" in `docs/ui-accessibility.md`). `inline` is for that mark in
 * running text: the glyph takes the text's size and baseline, as the inline notice's other glyphs
 * do, where elsewhere it keeps its own size. `text` is a word drawn after the glyph, for a cell whose
 * siblings are a glyph and a word: in the popover form the two together are the one trigger, named by
 * `label` all the same and drawn in the surrounding text's size and weight, and with no popover the
 * word follows the glyph in the same span, as plain text.
 *
 * `actions` is the popover's last line, the recovery button (Try again) of a failure that can be
 * retried, at the popover's trailing edge; it is given a function that closes the popover, to call as
 * the button is pressed. It applies to the popover form only.
 *
 * `inHeader` is for a mark in a collection's section header (the Git change list's section heading),
 * which the collection's own walk does not reach: the button takes no tab stop of its own
 * (`excludeFromTabOrder`), a press does not give it focus (`preventFocusOnPress`, or the collection
 * would send focus to its first row and scroll the list), it carries `data-section-heading-control`
 * for the application's own walk (`moveAcrossSectionHeadings`, Right from the heading's band reaches
 * it and Left goes back), and when its popover closes, by Escape or by Try again, focus goes to the
 * heading's band rather than to the mark. Only if focus is still where the close left it (on the
 * page body, the mark or inside the popover): a press outside that put focus on another control
 * keeps it there.
 *
 * The popover is portalled out of the row, so a surface that watches presses outside itself (the
 * rail's quota panel) tells this one by its `data-failure-mark-popover` attribute.
 *
 * Inside a tree row the press goes to this button and not to the row's own action.
 */
export function FailureMark({
  label,
  status = "danger",
  inline = false,
  inHeader = false,
  text,
  actions,
  children,
}: {
  /** What failed, the mark's accessible name. */
  label: string;
  status?: "danger" | "warning";
  inline?: boolean;
  /** The mark stands in a collection's section header; see above. */
  inHeader?: boolean;
  /** A visible word after the glyph; the accessible name stays `label`. */
  text?: string;
  actions?: (close: () => void) => React.ReactNode;
  children?: React.ReactNode;
}): React.ReactElement {
  const [open, setOpen] = useState(false);
  const [hovered, setHovered] = useState(false);
  const anchor = useRef<HTMLSpanElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  // Stable, and reading the trigger when it runs rather than while rendering, which `actions` is handed it in.
  const changeOpen = useStableHandler((next: boolean) => {
    setOpen(next);
    if (next || !inHeader) return;
    // The popover gives focus back to what held it as it unmounts, from a frame of its own, so the
    // heading's band takes it from the frame after that, unless the user has put focus elsewhere.
    const band = trigger.current?.closest("[role=row]")?.querySelector<HTMLElement>("[data-section-heading]");
    const popovers = [...document.querySelectorAll("[data-failure-mark-popover]")];
    requestAnimationFrame(() => requestAnimationFrame(() => {
      const active = document.activeElement;
      const unclaimed = !active || active === document.body || active === trigger.current || popovers.some((popover) => popover.contains(active));
      if (unclaimed && band?.isConnected) band.focus();
    }));
  });
  const Icon = status === "danger" ? CircleAlert : TriangleAlert;
  const glyph = <Icon aria-hidden="true" className={`${inline && !children ? "size-3.5 translate-y-0.5" : "size-4"} shrink-0 ${status === "danger" ? "text-danger-soft-foreground" : "text-warning-soft-foreground"}`} />;
  if (!children) {
    return (
      <>
        <span
          ref={anchor}
          role="img"
          aria-label={label}
          className={text ? "inline-flex items-center gap-1.5 whitespace-nowrap" : "inline-flex shrink-0 align-baseline"}
          onPointerEnter={() => setHovered(true)}
          onPointerLeave={() => setHovered(false)}
        >
          {glyph}
          {text}
        </span>
        {!text && <AnchoredTooltip tooltip={label} anchor={anchor} hovered={hovered} />}
      </>
    );
  }
  const header = inHeader ? { excludeFromTabOrder: true, preventFocusOnPress: true, "data-section-heading-control": "" } : undefined;
  return (
    <Popover isOpen={open} onOpenChange={changeOpen}>
      <TitledControl title={label} isDisabled={open}>
        {text ? (
          <Button
            ref={trigger}
            {...header}
            variant="ghost"
            aria-label={label}
            className="-mx-1 -my-0.5 h-auto min-h-0 w-auto min-w-0 shrink-0 gap-1.5 whitespace-nowrap rounded-full px-1 py-0.5 text-[length:inherit] leading-[inherit] font-normal text-inherit [&>svg]:m-0"
          >
            {glyph}
            {text}
          </Button>
        ) : (
          <Button ref={trigger} {...header} isIconOnly size="xs" variant="ghost" aria-label={label} className="shrink-0 -my-0.5 rounded-full">
            {glyph}
          </Button>
        )}
      </TitledControl>
      <Popover.Content placement="top" data-failure-mark-popover="" className="control-fills max-w-80">
        <Popover.Dialog aria-label={label} className="flex flex-col gap-2 p-3 text-sm [overflow-wrap:anywhere]">
          {children}
          {actions && <div className="flex justify-end">{actions(() => changeOpen(false))}</div>}
        </Popover.Dialog>
      </Popover.Content>
    </Popover>
  );
}
