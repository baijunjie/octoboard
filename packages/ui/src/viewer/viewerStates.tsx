// The lines that stand in for content the viewer is not showing: a notice saying why it is shown the
// way it is, a change with nothing to draw, and the spinner while it is on its way.
import { Spinner } from "@heroui/react";
import React from "react";

import { useT } from "../i18n/react";

/** A line saying why the content below is shown the way it is. */
export function Notice({ children }: { children: React.ReactNode }): React.ReactElement {
  return <p className="shrink-0 text-xs text-muted">{children}</p>;
}

/** The line standing in for a change with nothing to draw: no patch, or a patch with no lines. */
export function Unreadable(): React.ReactElement {
  const t = useT();
  return <p className="text-sm text-muted">{t("viewer.change.unreadable")}</p>;
}

/** Visual only. The viewer's loading is spoken from its own status region, so this overlay, inserted
 * already holding its text, stays silent; only the renderer's pause while it draws a large file's
 * code goes unspoken. */
export function Loading(): React.ReactElement {
  const t = useT();
  return (
    <div aria-hidden="true" className="flex min-h-0 flex-1 items-center justify-center gap-2 text-sm text-muted">
      <Spinner size="sm" aria-hidden="true" />
      {t("viewer.loading")}
    </div>
  );
}

/** `Loading` laid over the frame that holds the content, for it to be centred in the frame (it only
 * centres itself inside a flex parent). It goes over the content rather than beside it, so taking it
 * away does not lay out a large file a second time. The frame is `relative`. */
export function LoadingOverlay(): React.ReactElement {
  return (
    <div className="absolute inset-0 flex">
      <Loading />
    </div>
  );
}
