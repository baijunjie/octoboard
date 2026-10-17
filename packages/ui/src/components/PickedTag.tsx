import { Tag } from "@heroui/react";
import React from "react";

import { textDirection } from "../i18n/languages";
import { useCurrentLanguage, useT } from "../i18n/react";
import { FadeOverflow } from "./FadeOverflow";
import { TitledControl } from "./TitledControl";

/** HeroUI has no accent variant for a removable tag, so this uses its selected tag's colour tokens
 * without introducing selection behaviour. The 3rem minimum applies only while the remove button
 * is visible, keeping a short label clear of the button and its fade while fitting the content at rest. */
const TAG_CLASS =
  "group/tag min-w-0 hover:min-w-12 data-focus-visible:min-w-12 max-w-full bg-accent-soft text-accent-soft-foreground hover:bg-accent-soft-hover";

/** Laid over the tag's end rather than in the row, so it does not reserve space in the row:
 * 0.25rem from the end, inside the tag's 0.5rem end padding, so the 0.75rem glyph
 * covers the label's last 0.5rem (`LABEL_CLASS`'s fade is measured from these). Shown while the tag
 * is hovered or has keyboard focus. `group-hover` rather than react-aria's `data-hovered`: react-aria
 * leaves hover tracking off on a tag that is only removable, so the tag never gets that attribute.
 * `bg-transparent` cancels the button's own fill so the tag's hover fill is not left with a pill
 * inside it. */
const REMOVE_CLASS =
  "absolute inset-y-0 end-1 my-auto opacity-0 transition-opacity duration-100 motion-reduce:transition-none group-hover/tag:opacity-100 group-data-focus-visible/tag:opacity-100 bg-transparent";

/** The glyph and its hover fill are drawn from the tag's own foreground. */
const REMOVE_COLOUR_CLASS =
  "text-accent-soft-foreground/80 hover:bg-accent-soft-foreground/15 hover:text-accent-soft-foreground";

/** While the remove button shows, the label fades out ahead of it: opaque until 0.75rem before the
 * button's edge, fading to transparent at it, toward the tag's end, where the button sits.
 * The direction is the tag's own (`tagDirection`), set on the tag, which the `rtl:` variant cannot
 * follow: it also matches anything under the right-to-left `<html>`. `flex-1` makes the box reach
 * the tag's end, where the button is, so the fade is measured from there and a short label in a
 * tag held wider by the active minimum stays clear of it. The mask is on an element around
 * `FadeOverflow`, whose own fade at a cut edge is a mask of its own. */
const LABEL_CLASS =
  "flex min-w-0 flex-1 [--picked-tag-fade:linear-gradient(to_right,#000_calc(100%_-_1.25rem),transparent_calc(100%_-_0.5rem))] group-[[dir=rtl]]/tag:[--picked-tag-fade:linear-gradient(to_left,#000_calc(100%_-_1.25rem),transparent_calc(100%_-_0.5rem))] group-hover/tag:[mask-image:var(--picked-tag-fade)] group-data-focus-visible/tag:[mask-image:var(--picked-tag-fade)]";

/** The scripts written right to left that a tag is likely to be in; rarer ones (Samaritan,
 * Mandaic, Adlam, Hanifi Rohingya) are left out, and such a tag takes the left-to-right layout. */
const RIGHT_TO_LEFT_LETTER = /[\p{Script=Hebrew}\p{Script=Arabic}\p{Script=Syriac}\p{Script=Thaana}\p{Script=Nko}]/u;

/** The direction a tag's text runs in, from its first letter (as `dir="auto"` decides), or the UI's
 * when it has none. The whole tag takes it, so the remove button sits at the end the text runs to
 * and never covers its beginning, even when the text's direction is not the UI's (a Latin tag in
 * the Arabic UI). */
function tagDirection(tag: string, ui: "ltr" | "rtl"): "ltr" | "rtl" {
  const letter = tag.match(/\p{L}/u)?.[0];
  if (letter === undefined) return ui;
  return RIGHT_TO_LEFT_LETTER.test(letter) ? "rtl" : "ltr";
}

/**
 * A tag the user has picked, removable, in the accent look. It goes in a HeroUI `TagGroup`, whose
 * `onRemove` receives its text. The label is one line that fades out where it is cut off and, while
 * the pointer is over the tag, scrolls through its whole text. The remove button shows only while
 * the pointer is over the tag or the tag has keyboard focus (where Backspace and Delete also remove
 * it), and then covers the label's end (the end its text runs to, see `tagDirection`), which fades
 * out ahead of it, so short tags widen only while the button is visible.
 */
export function PickedTag({
  tag,
  removeTitle,
}: {
  tag: string;
  /** The remove button's tooltip, naming the tag. Its accessible name is the bare verb; the row adds
   * the tag's own text to it through react-aria's `aria-labelledby`, so it is not announced twice. */
  removeTitle: string;
}): React.ReactElement {
  const t = useT();
  const dir = tagDirection(tag, textDirection(useCurrentLanguage()));
  return (
    <Tag
      id={tag}
      data-list-motion={`tag:${tag}`}
      dir={dir}
      textValue={tag}
      data-marquee-scope
      className={TAG_CLASS}
    >
      {/* The function form: TagRoot only finds a remove button among its direct children, and would add
          a default one beside a wrapped button. */}
      {() => (
        <>
          <span className={LABEL_CLASS}>
            {/* A cut label fades at its end and runs as a marquee while the pointer is over the tag
                (`data-marquee-scope`). While the remove button shows, the spacer after the text,
                positioned so it adds to the scrollable width but not to the tag's, lets the marquee
                carry the text's end out from under the button and its fade: a label whose end the
                button covers is then cut, and scrolls until that end is clear; one that ends well
                short of the button (in a tag held wider by the active minimum) does not move. The title
                is for where the marquee does not run (reduced motion); being cut on hover, a label that
                fits also gets it then. */}
            <FadeOverflow as="span" className="min-w-0 flex-1" titleWhenClipped={tag}>
              <span className="relative">
                {tag}
                <span aria-hidden="true" className="absolute start-full top-0 hidden h-px w-5 group-hover/tag:block" />
              </span>
            </FadeOverflow>
          </span>
          <TitledControl title={removeTitle}>
            <Tag.RemoveButton
              aria-label={t("common.removeTag")}
              className={`${REMOVE_CLASS} ${REMOVE_COLOUR_CLASS}`}
            />
          </TitledControl>
        </>
      )}
    </Tag>
  );
}
