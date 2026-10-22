import React, { useLayoutEffect, useRef, useState } from "react";

import { ringForAutomaticFocus } from "../touchInput";
import { useLatest } from "../components/useLatest";
import { ArchiveView } from "./ArchiveView";

type View = React.ComponentProps<typeof ArchiveView>;

/** Keeps the last archive on screen for its exit without delaying selection or navigation. */
export function ArchiveLayer({ view, focusTerminal }: { view: View | undefined; focusTerminal: () => void }): React.ReactElement | null {
  const [retained, setRetained] = useState(view);
  const open = view !== undefined;
  const rootRef = useRef<HTMLDivElement>(null);
  const latestFocusTerminal = useLatest(focusTerminal);

  useLayoutEffect(() => {
    if (view) setRetained(view);
  }, [view]);

  useLayoutEffect(() => {
    if (open || !retained) return;
    if (rootRef.current?.contains(document.activeElement) || document.activeElement === document.body) {
      ringForAutomaticFocus();
      latestFocusTerminal.current();
    }
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)");
    const finish = () => setRetained(undefined);
    if (reduced.matches) {
      finish();
      return;
    }
    // A backgrounded tab or a skipped animation event must not retain the archive indefinitely.
    const fallback = setTimeout(finish, 290);
    const root = rootRef.current;
    const ended = (event: AnimationEvent) => {
      if (event.animationName === "archive-exit" && (event.target as HTMLElement).matches("[data-region=archive]")) finish();
    };
    root?.addEventListener("animationend", ended);
    const changed = () => {
      if (reduced.matches) finish();
    };
    reduced.addEventListener("change", changed);
    return () => {
      clearTimeout(fallback);
      root?.removeEventListener("animationend", ended);
      reduced.removeEventListener("change", changed);
    };
  }, [open, retained, latestFocusTerminal]);

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
