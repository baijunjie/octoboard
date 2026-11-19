import React, { useId } from "react";

/** A card on the content panel, with a small heading above it, which the form's controls sit on: the panel's own
 * ground is too near the fields' to set them off, so they lie on the raised card surface, as controls on a surface do.
 * `title` is left out for a card that has its own words. */
export function FormCard({ title, children }: { title?: string; children: React.ReactNode }): React.ReactElement {
  const id = useId();
  return (
    <section aria-labelledby={title ? id : undefined} className="flex flex-col gap-2">
      {title && <h2 id={id} className="text-sm font-medium text-app-muted">{title}</h2>}
      <div className="card-fills @container flex flex-col gap-5 rounded-lg border border-transparent bg-(--panel-row) p-4 shadow-(--panel-shadow)">{children}</div>
    </section>
  );
}
