import React from "react";

/** A small neutral pill after a name, carrying a value or qualifier of it: a status ("Not added"), the
 * account a session runs under, an agent, the value an unset field inherits, the size of a context.
 * Rounded and never shrinking (unless `overflow` says otherwise), so the name beside it is what
 * gives way when the room runs out. Its fill is a tint of the secondary text colour over whatever it sits on,
 * with a hairline edge of the same tint, so it reads as a pill on every surface it is drawn on: a
 * sidebar row resting, hovered and selected, the Focus cards and the archive's rows, a tooltip
 * (also the dark one over a dialog), a field, a dialog and a popover, in light and dark (a `--default`
 * fill is the field's own grey). Its text is the foreground colour at 80%, which keeps 4.5:1 on that
 * fill where the secondary text colour fell to 3.6:1 on a field: measured over the fill on each of
 * those surfaces it stays above 6.5:1 in light and 5.9:1 in dark, at the worst on a hovered
 * select's fill, and above 8:1 on a dialog, popover or tooltip. A comma that only assistive technology reads keeps the name and the value apart, which
 * `separated={false}` leaves out for a badge with no name before it. `overflow` lets the badge
 * shrink for text that may be long: `"wrap"` breaks it inside a long word (a path in a tooltip,
 * which cannot run past its width). `dir` is the direction of that text where it is not the UI's. */
export function StatusBadge({ children, className = "", separated = true, overflow, dir }: {
  children: React.ReactNode;
  className?: string;
  separated?: boolean;
  overflow?: "wrap";
  dir?: "ltr" | "auto";
}): React.ReactElement {
  return (
    <>
      {separated && <span className="sr-only">, </span>}
      <span dir={dir} className={`${overflow === "wrap" ? "min-w-0 rounded-lg [overflow-wrap:anywhere]" : "shrink-0 whitespace-nowrap rounded-full"} bg-[color-mix(in_oklab,var(--app-muted)_16%,transparent)] px-2 py-0.5 text-xs leading-4 text-[color-mix(in_oklab,var(--foreground)_80%,transparent)] ring-1 ring-inset ring-[color-mix(in_oklab,var(--app-muted)_28%,transparent)] ${className}`}>{children}</span>
    </>
  );
}
