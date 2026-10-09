import { Tag } from "@heroui/react";
import React from "react";

import { textDirection } from "../i18n/languages";
import { useCurrentLanguage, useT } from "../i18n/react";
import { FadeOverflow } from "./FadeOverflow";
import { TitledControl } from "./TitledControl";

/** The accent look: HeroUI's soft accent fill, the one it gives a selected `Tag`. That fill is a translucent tint, so
 * on its own it would take on whatever lies behind it — the tags field's grey, which also darkens on
 * hover, would pull its text under 4.5:1 in the light theme. It is laid as an image over an opaque
 * `--surface` colour instead, so the tag reads the same wherever it sits and its contrast is the one
 * measured on `--surface`. Hover swaps in HeroUI's hover tint. The 3rem minimum keeps a short label's
 * first characters clear of the remove button and its fade, at rest as well as on hover, so a tag
 * never widens under the pointer and pushes its neighbours or wraps onto the next line. */
const ACCENT_TAG_CLASS =
  "group/tag min-w-12 max-w-full bg-surface bg-[linear-gradient(var(--accent-soft),var(--accent-soft))] text-accent-soft-foreground hover:bg-[linear-gradient(var(--accent-soft-hover),var(--accent-soft-hover))]";

/** The neutral look: HeroUI's default tag fill, which in the sidebar is its own fill
 * (`.sidebar-fills`). The minimum width is the accent look's, for the same reason. */
const NEUTRAL_TAG_CLASS = "group/tag min-w-12 max-w-full";

/** Laid over the tag's end rather than in the row, so the tag keeps one width whether the button
 * shows or not: 0.25rem from the end, inside the tag's 0.5rem end padding, so the 0.75rem glyph
 * covers the label's last 0.5rem (`LABEL_CLASS`'s fade is measured from these). Shown while the tag
 * is hovered or has keyboard focus. `group-hover` rather than react-aria's `data-hovered`: react-aria
 * leaves hover tracking off on a tag that is only removable, so the tag never gets that attribute.
 * `bg-transparent` cancels the button's own fill so the tag's hover fill is not left with a pill
 * inside it. */
const REMOVE_CLASS =
  "absolute inset-y-0 end-1 my-auto opacity-0 transition-opacity duration-100 motion-reduce:transition-none group-hover/tag:opacity-100 group-data-focus-visible/tag:opacity-100 bg-transparent";

/** The glyph and its hover fill are drawn from each look's own foreground: HeroUI's
 * `bg-default-hover` is a translucent fill about as strong as the neutral tag's own and so would
 * add little on it. */
const ACCENT_REMOVE_CLASS =
  "text-accent-soft-foreground/80 hover:bg-accent-soft-foreground/15 hover:text-accent-soft-foreground";
const NEUTRAL_REMOVE_CLASS = "text-foreground/80 hover:bg-foreground/10 hover:text-foreground";

/** While the remove button shows, the label fades out ahead of it: opaque until 0.75rem before the
 * button's edge, fading to transparent at it, toward the tag's end, where the button sits.
 * The direction is the tag's own (`tagDirection`), set on the tag, which the `rtl:` variant cannot
 * follow: it also matches anything under the right-to-left `<html>`. `flex-1` makes the box reach
 * the tag's end, where the button is, so the fade is measured from there and a short label in a
 * tag held wider by `min-w-12` stays clear of it. The mask is on an element around `FadeOverflow`,
 * whose own fade at a cut edge is a mask of its own. */
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
 * A tag the user has picked, removable, in the accent look; the neutral one sets apart a filter's
 * keyword shown beside its tags. It goes in a HeroUI `TagGroup`, whose `onRemove` receives its
 * `id`. The label is one line that fades out where it is cut off and, while the pointer is over the
 * tag, scrolls through its whole text. The remove button shows only while the pointer is over the
 * tag or the tag has keyboard focus (where Backspace and Delete also remove it), and then covers the
 * label's end (the end its text runs to, see `tagDirection`), which fades out ahead of it, so
 * showing it never changes the tag's width.
 */
export function PickedTag({
  tag,
  id = tag,
  removeTitle,
  removeLabel,
  tone = "accent",
  preventFocusOnPress,
}: {
  tag: string;
  /** The key `onRemove` receives; the tag's text by default. A tag whose text changes in place (a
   * keyword being typed) needs a fixed one: react-aria throws when an item's id changes. */
  id?: string;
  /** The remove button's tooltip, naming the tag. */
  removeTitle: string;
  /** The remove button's accessible name, the bare verb ("Remove tag" by default); the row adds the
   * tag's own text to it through react-aria's `aria-labelledby`, so it is not announced twice. */
  removeLabel?: string;
  tone?: "accent" | "neutral";
  /** Keeps a press on the remove button from taking focus (off the terminal). */
  preventFocusOnPress?: boolean;
}): React.ReactElement {
  const t = useT();
  const dir = tagDirection(tag, textDirection(useCurrentLanguage()));
  return (
    <Tag
      id={id}
      dir={dir}
      textValue={tag}
      data-marquee-scope
      className={tone === "accent" ? ACCENT_TAG_CLASS : NEUTRAL_TAG_CLASS}
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
                short of the button (in a tag held wider by `min-w-12`) does not move. The title is
                for where the marquee does not run (reduced motion); being cut on hover, a label that
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
              aria-label={removeLabel ?? t("common.removeTag")}
              preventFocusOnPress={preventFocusOnPress}
              className={`${REMOVE_CLASS} ${tone === "accent" ? ACCENT_REMOVE_CLASS : NEUTRAL_REMOVE_CLASS}`}
            />
          </TitledControl>
        </>
      )}
    </Tag>
  );
}
