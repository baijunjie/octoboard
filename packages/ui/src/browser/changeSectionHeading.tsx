// What both layouts of the Git mode's change list put above a section's changes: the band itself,
// the control that folds the section away and unfolds it, and the key handling that reaches that
// control, which the collections the lists are built on do not reach themselves.
import { ChevronDown, ChevronRight } from "lucide-react";
import React, { useRef } from "react";
import { Button } from "react-aria-components";

import { handFocusOff } from "../components/handFocusOff";
import { useCurrentLanguage, useT } from "../i18n/react";
import { SECTION_LABELS, sectionKey, type ChangeSection } from "./changes";

/** Every section heading is one line of this height, which, with the rows' (`ROW_HEIGHT`), is what
 * lets the list be virtualized, flat or as a tree. */
export const HEADING_HEIGHT = 30;
/** A section's heading, in both layouts, a band of HeroUI's tertiary surface so a section reads as a
 * step off the panel: 1.15:1 from it in light and 1.10:1 in dark. `h-[30px]` is `HEADING_HEIGHT`.
 * What the band holds is one control filling it (`ChangeSectionHeading`), so the line's own layout and
 * padding sit there rather than here. */
export const HEADING_CLASS = "flex h-[30px] bg-surface-tertiary text-xs font-medium text-muted";

/** What a section's heading says: the section's name and how many changes it holds. */
function SectionHeadingText({ section, count }: { section: ChangeSection; count: number }): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  return (
    <>
      {t(SECTION_LABELS[section])}
      <span>{new Intl.NumberFormat(language).format(count)}</span>
    </>
  );
}

/**
 * Keeps a heading's own control focused across a fold, which it holds after one from the keyboard,
 * and after the hand-off in `ChangeSectionHeading` below has sent a row's focus there. react-aria answers
 * the row that held its focused key going away by moving focus to the nearest row the list has left,
 * and the user's place is the heading they just pressed rather than a row of another section. It
 * does that from an effect of the collection's, above this component's own, so the two renders it
 * takes have to be out of the way before the heading takes focus back.
 *
 * Only while focus is still inside the list, so a Tab or a press elsewhere in between stands, as
 * `focusRow` keeps the place the user went to.
 */
function keepHeadingFocus(band: HTMLButtonElement | null): void {
  const list = band?.closest<HTMLElement>("[role=grid], [role=treegrid]");
  if (!band || !list || document.activeElement !== band) return;
  requestAnimationFrame(() =>
    requestAnimationFrame(() => {
      if (document.activeElement !== band && list.contains(document.activeElement)) band.focus();
    }),
  );
}

/**
 * What a section's heading holds, in both layouts: one button filling the band, which folds the
 * section's rows away and unfolds them again (its key in `collapsed` is `sectionKey`), showing the
 * chevron a folder of the Files tree shows, the section's name and its count.
 *
 * The button is the whole band rather than a chevron inside it, which settles three things at once:
 * a press anywhere on the heading folds the section, the pointer target is the band itself rather
 * than a 14-pixel glyph (the "Pointer target size" section of `docs/ui-accessibility.md`), and the
 * name assistive technology announces is the band's own text, so no label of its own has to repeat
 * the count beside the section's name (the same document's "Accessible names"). It is a react-aria
 * button rather than HeroUI's because HeroUI's brings a fill, a size and a padding of its own, where
 * this one has to be the band. It carries no tooltip, the band saying in text what a folder's
 * chevron needs one for; which state it is in reaches assistive technology through `aria-expanded`
 * and the eye through the chevron. The band answers a hover with nothing, where every row of the
 * lists tints: the chevron is the affordance here, as it is on a folder of the Files tree, and a
 * fill over `surface-tertiary` is a contrast pair nobody has measured. Measure it before adding
 * one.
 *
 * It is no tab stop of its own, as a control inside a collection's row is none: the list is one tab
 * stop, and `moveAcrossSectionHeadings`, which finds it by the `data-section-heading` it carries, is
 * what reaches it from the keyboard. Nor may a
 * press take focus — react-aria's collection answers focus landing anywhere inside it that is not
 * one of its rows by moving focus to its first row, so a heading that took focus on a mouse press
 * would scroll the list back to the top and leave focus there. Only the running page shows that
 * one: jsdom focuses nothing on a press of its own, as the "Focus dropped to `<body>`" section of
 * `docs/memory/focus-and-key-handling-in-the-ui.md` has it.
 */
export function ChangeSectionHeading({
  section,
  count,
  collapsed,
  onCollapsedChange,
}: {
  section: ChangeSection;
  count: number;
  /** The row keys folded away, the heading's own among them when the section is folded. */
  collapsed: ReadonlySet<string>;
  onCollapsedChange: (collapsed: ReadonlySet<string>) => void;
}): React.ReactElement {
  const button = useRef<HTMLButtonElement>(null);
  const key = sectionKey(section);
  const isCollapsed = collapsed.has(key);
  const toggle = () => {
    const next = new Set(collapsed);
    if (isCollapsed) next.delete(key);
    else {
      next.add(key);
      // Folding takes every row of the section away, and one of them may hold keyboard focus. It
      // goes to the heading, which stands whenever its rows do, and the ring follows how the press
      // came, this press being the trigger ("Focus moved by a change the user did not make" in
      // `docs/ui-accessibility.md`). `handFocusOff` is given the band rather than the button itself,
      // whose tab index is -1, so that it reaches the button inside it.
      handFocusOff(button.current?.closest("[role=rowgroup]"), button.current?.closest<HTMLElement>("[role=row]"), false);
      keepHeadingFocus(button.current);
    }
    onCollapsedChange(next);
  };
  return (
    <Button
      ref={button}
      data-section-heading
      aria-expanded={!isCollapsed}
      excludeFromTabOrder
      preventFocusOnPress
      onPress={toggle}
      className="flex h-full w-full min-w-0 items-center gap-2 px-3 text-start outline-none data-[focus-visible]:ring-2 data-[focus-visible]:ring-focus data-[focus-visible]:ring-inset"
    >
      {/* The chevron points the way the Files tree's does, and mirrors under right-to-left while
          it points along the text. */}
      {isCollapsed ? (
        <ChevronRight aria-hidden="true" className="size-3.5 shrink-0 rtl:-scale-x-100" />
      ) : (
        <ChevronDown aria-hidden="true" className="size-3.5 shrink-0" />
      )}
      <SectionHeadingText section={section} count={count} />
    </Button>
  );
}

/**
 * Up and Down across a section heading, for the capture phase of an element around a change list.
 *
 * A react-aria collection manages focus itself: the list is one tab stop, Up and Down walk its
 * rows, and a control inside a row is reached by the arrow key that runs along the row. A section
 * heading is no row of the collection, and none of that reaches it — in react-aria-components
 * 1.21.1 the keyboard delegate walks items alone, Tab inside the collection jumps to the last
 * tabbable element and leaves it, and focus landing inside the list anywhere but on a row is moved
 * to the list's first row (`ChangeList.test.tsx` pins the last of those). So the heading joins the
 * walk here: the rows and the headings between them are the stops of Up and Down, which is the same
 * model one step wider.
 *
 * This has to run above the rows, because a row answers Up and Down in a capture-phase handler of
 * its own. A move between two rows is left to the collection; only a step onto a heading, or off
 * one, is taken here. Where the next stop is outside what the virtualizer has rendered the key is
 * left to the collection too, which scrolls and renders as it moves, and the heading is then
 * reached by stepping back onto it. The collection moves from its own focused key, which a heading
 * taking focus never updates: for a heading this walk arrived at, that key is the row it last stood
 * on, which lies on the far side of the heading from where the walk is going, so the collection's
 * own move steps over the heading and lands on the stop beyond it; at the ends of the list it has
 * nowhere to go and does nothing. For a heading that holds focus because folding it took the
 * focused row away, the key is wherever react-aria moved it, and where that stop also lies outside
 * what the virtualizer has rendered — both at once, and rare twice over — the move is the
 * collection's to make.
 *
 * react-aria tracks the interaction modality from a capture-phase listener on the document, which
 * runs before this one, so what this focuses draws its ring without `setInteractionModality`.
 */
function moveAcrossSectionHeadings(event: React.KeyboardEvent<HTMLElement>): void {
  if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
  const target = event.target as HTMLElement;
  const stop = target.closest<HTMLElement>("[role=row]");
  if (!stop) return;
  const stops = [...event.currentTarget.querySelectorAll<HTMLElement>("[role=row]")];
  const from = stops.indexOf(stop);
  if (from === -1) return;
  const next = stops[from + (event.key === "ArrowDown" ? 1 : -1)];
  const heading = next?.querySelector<HTMLElement>("[data-section-heading]");
  if (!next || (heading === null && target.closest("[data-section-heading]") === null)) return;
  event.preventDefault();
  event.stopPropagation();
  (heading ?? next).focus();
  // The list's `scroll-padding` is the size of its fade (`useScrollFade`), so this lands clear of
  // it. jsdom implements no `scrollIntoView`, hence the call through `?.`.
  next.scrollIntoView?.({ block: "nearest" });
}

/** A change list, inside the key handling that makes its section headings stops of the Up and Down
 * walk (`moveAcrossSectionHeadings`). It lays nothing out (`display: contents`), so the list itself
 * stays the pane's flex item and the scroll container of its own rows. */
export function ChangeSectionHeadingStops({ children }: { children: React.ReactNode }): React.ReactElement {
  return (
    <div className="contents" onKeyDownCapture={moveAcrossSectionHeadings}>
      {children}
    </div>
  );
}
