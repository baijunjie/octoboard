import React from "react";

/** A small grey badge after a name, saying what is the matter with it ("Not added"): rounded, secondary
 * text, never shrinking, so the name beside it is what gives way when the room runs out. Its fill is a tint
 * of the secondary text colour over whatever it sits on, with a hairline edge of the same tint, so it reads
 * as a pill on a field, a dialog and a popover alike, in light and dark (a `--default` fill is the field's
 * own grey). Its text is the foreground colour at 80%, which keeps 4.5:1 on that fill where the secondary
 * text colour fell to 3.6:1 on a field. A comma that only assistive technology reads keeps the name and
 * the status apart. */
export function StatusBadge({ children, className = "" }: { children: React.ReactNode; className?: string }): React.ReactElement {
  return (
    <>
      <span className="sr-only">, </span>
      <span className={`shrink-0 whitespace-nowrap rounded-full bg-[color-mix(in_oklab,var(--app-muted)_16%,transparent)] px-2 py-0.5 text-xs leading-4 text-[color-mix(in_oklab,var(--foreground)_80%,transparent)] ring-1 ring-inset ring-[color-mix(in_oklab,var(--app-muted)_28%,transparent)] ${className}`}>{children}</span>
    </>
  );
}
