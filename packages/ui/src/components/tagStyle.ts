/** The look every removable tag the user has picked shares, in the project dialog's Tags field and
 * on the sidebar's filter heading: HeroUI's soft accent fill, the one it gives a selected `Tag`.
 * That fill is a translucent tint, so on its own it would take on whatever lies behind it — the
 * field's grey, which also darkens on hover, would pull its text under 4.5:1 in the light theme.
 * It is laid as an image over an opaque `--surface` colour instead, so the tag reads the same
 * wherever it sits and its contrast is the one measured on `--surface`. Hover swaps in HeroUI's
 * hover tint. */
export const PICKED_TAG_CLASS =
  "max-w-full bg-surface bg-[linear-gradient(var(--accent-soft),var(--accent-soft))] text-accent-soft-foreground hover:bg-[linear-gradient(var(--accent-soft-hover),var(--accent-soft-hover))]";

/** The remove button inside a `PICKED_TAG_CLASS` tag: the glyph and its hover fill are drawn from
 * the soft fill's own foreground, and `bg-transparent` cancels the button's own fill so the tag's
 * hover fill is not left with a pill inside it. */
export const PICKED_TAG_REMOVE_CLASS =
  "bg-transparent text-accent-soft-foreground/80 hover:bg-accent-soft-foreground/15 hover:text-accent-soft-foreground";
