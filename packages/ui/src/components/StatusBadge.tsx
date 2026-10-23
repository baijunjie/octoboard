import React from "react";

export type StatusBadgeColor = "default" | "accent" | "help" | "success" | "warning" | "danger";
export type StatusBadgeVariant = "soft" | "bordered";

/* Fill, text and edge are the shared tints of style.css (`--tint-{colour}-bg`, `-fg` and `-edge`), the same
   colours an `InlineReference` is drawn from; the edge is the bordered variant's hairline and the chip never takes it.
   The neutral badge's text is HeroUI's `--default-foreground`, which style.css has no token for.
   The classes are spelled out, since Tailwind finds them by their text. */
const COLORS: Record<StatusBadgeColor, { soft: string; ring: string }> = {
  default: { soft: "bg-[var(--tint-default-bg)] text-[var(--default-foreground)]", ring: "ring-[var(--tint-default-edge)]" },
  accent: { soft: "bg-[var(--tint-accent-bg)] text-[var(--tint-accent-fg)]", ring: "ring-[var(--tint-accent-edge)]" },
  help: { soft: "bg-[var(--tint-help-bg)] text-[var(--tint-help-fg)]", ring: "ring-[var(--tint-help-edge)]" },
  success: { soft: "bg-[var(--tint-success-bg)] text-[var(--tint-success-fg)]", ring: "ring-[var(--tint-success-edge)]" },
  warning: { soft: "bg-[var(--tint-warning-bg)] text-[var(--tint-warning-fg)]", ring: "ring-[var(--tint-warning-edge)]" },
  danger: { soft: "bg-[var(--tint-danger-bg)] text-[var(--tint-danger-fg)]", ring: "ring-[var(--tint-danger-edge)]" },
};

const RING = "ring-1 ring-inset";

function colorClasses(color: StatusBadgeColor, variant: StatusBadgeVariant): string {
  const { soft, ring } = COLORS[color];
  return variant === "soft" ? soft : `${soft} ${RING} ${ring}`;
}

/** A small pill after a name, carrying a state or qualifier of it: a status ("Not added", "Logged
 * in"), the account a session runs under, an agent, the value an unset field inherits, the size of a
 * context. Rounded and never shrinking (unless `overflow` says otherwise), so the name beside it is
 * what gives way when the room runs out.
 *
 * `color` is one of the shared tints of style.css (`--tint-{colour}-bg`, `-fg`, `-edge`: neutral, HeroUI's four and
 * the app's purple `help`, for help and hints), the colours an `InlineReference` is drawn from too. `variant` is
 * `"soft"` (borderless, for a state) or `"bordered"` (a hairline edge, for a qualifier of the name; the default).
 *
 * The neutral badge is the foreground at 8% in light and 10% in dark, with `--default-foreground` as text and a
 * hairline of `--app-muted` at 28%. The fill is translucent, as the coloured ones are, so it deepens on a grey
 * surface (a hovered or selected row, an Alert) instead of vanishing into it. A coloured badge has HeroUI's soft fill
 * (12-15% of the colour) and, bordered, a hairline mixed 35% from the solid colour; HeroUI's soft foreground alone
 * falls to 3.5:1-4.3:1 on some surfaces, so the text is mixed toward the foreground (15% in light for accent, warning
 * and danger, 20% for success; 15% for accent and 25% for danger in dark, none for success and warning). Measured
 * with the fill composited over the field, dialog and popover surface, a tooltip over a dialog and its control fill,
 * the dialog's and the pane's control fill and its hover, the sidebar row resting, hovered and selected, `--panel`
 * with its hover and selected fills, `--background`, `--panel-sidebar` and `--chrome`, the Git change list's rows
 * (the sidebar's surface), a card on `--surface` and, for the neutral badge, a neutral Alert's fill, the text minima
 * are, in light, 10.68:1 neutral, 4.51:1 accent, 4.65:1 success, 4.65:1 warning, 4.75:1 danger and 6.45:1 help, at
 * the worst on a hovered control fill (the neutral fill read by the focus view's count badge on the sidebar's hovered
 * tertiary button is 9.42:1); in dark 7.26:1 neutral, 4.61:1 accent, 5.17:1 success, 5.67:1 warning, 4.70:1
 * danger and 5.78:1 help, at the worst on the dark tooltip's control fill. The fills sit 1.07:1-1.25:1 off a light
 * surface and 1.09:1-1.32:1 off a dark one (the neutral one 1.19:1-1.20:1 and 1.14:1-1.31:1), and the hairline
 * 1.15:1-1.52:1 from its fill in light and 1.28:1-2.33:1 in dark (neutral 1.38:1-1.48:1 and 1.36:1-1.58:1).
 *
 * A comma that only assistive technology reads keeps the name and the value apart, which
 * `separated={false}` leaves out for a badge with no name before it. `single` is for one character
 * (a change's status letter): a fixed width with no side padding, so the letter is centred in either
 * reading direction. `overflow` lets the badge shrink for text that may be long: `"wrap"` breaks it
 * inside a long word (a path in a tooltip, which cannot run past its width). `dir` is the direction of
 * that text where it is not the UI's. The other props, a `ref` among them, go to the badge's span. */
export function StatusBadge({ children, className = "", color = "default", variant = "bordered", separated = true, single = false, overflow, dir, ...rest }: Omit<React.ComponentProps<"span">, "color"> & {
  children?: React.ReactNode;
  className?: string;
  color?: StatusBadgeColor;
  variant?: StatusBadgeVariant;
  separated?: boolean;
  single?: boolean;
  overflow?: "wrap";
  dir?: "ltr" | "auto";
}): React.ReactElement {
  return (
    <>
      {separated && <span className="sr-only">, </span>}
      <span {...rest} dir={dir} data-color={color} data-variant={variant} className={`${overflow === "wrap" ? "min-w-0 rounded-lg [overflow-wrap:anywhere]" : "shrink-0 whitespace-nowrap rounded-full"} ${colorClasses(color, variant)} ${single ? "inline-block w-5 px-0 text-center" : "px-2"} py-0.5 text-xs leading-4 ${className}`}>{children}</span>
    </>
  );
}
