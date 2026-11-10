import React, { useLayoutEffect, useRef, useState } from "react";

import { onReducedMotion, prefersReducedMotion } from "../components/motion/reducedMotion";
import { ArchiveView } from "./ArchiveView";

/** How long the exit waits for its animation to end before it removes the view anyway. */
export const EXIT_FALLBACK_MS = 290;

type View = React.ComponentProps<typeof ArchiveView>;

/** Keeps the last archive on screen for its exit without delaying selection or navigation. Focus held in the
 * closing view is handed on by `useArchiveView`, not here. */
export function ArchiveLayer({ view }: { view: View | undefined }): React.ReactElement | null {
  const [retained, setRetained] = useState(view);
  const open = view !== undefined;
  const rootRef = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    if (view) setRetained(view);
  }, [view]);

  useLayoutEffect(() => {
    if (open || !retained) return;
    const finish = () => setRetained(undefined);
    if (prefersReducedMotion()) {
      finish();
      return;
    }
    // A backgrounded tab or a skipped animation event must not retain the archive indefinitely.
    const fallback = setTimeout(finish, EXIT_FALLBACK_MS);
    const root = rootRef.current;
    const ended = (event: AnimationEvent) => {
      if (event.animationName === "archive-exit" && (event.target as HTMLElement).matches("[data-region=archive]")) finish();
    };
    root?.addEventListener("animationend", ended);
    const stopReduced = onReducedMotion(finish);
    return () => {
      clearTimeout(fallback);
      root?.removeEventListener("animationend", ended);
      stopReduced();
    };
  }, [open, retained]);

  const shown = view ?? retained;
  if (!shown) return null;
  return (
    <div
      ref={rootRef}
      className="archive-layer absolute inset-0 z-10 overflow-hidden"
      data-state={open ? "open" : "closing"}
      inert={!open}
      aria-hidden={!open || undefined}
    >
      <ArchiveView
        {...shown}
        key={`${shown.console.id}:${shown.project?.id ?? ""}:${shown.boundTo?.id ?? ""}`}
        active={open}
      />
    </div>
  );
}
