import React, { useRef } from "react";

import { useFocusHandoff } from "../components/useFocusHandoff";

/** Keeps keyboard focus inside a confirmation when a control it wraps goes away while focused — an
 * option a completed inspection takes away: focus goes back to the confirmation itself. Renders no
 * box of its own. */
export function KeepFocusInDialog({ children }: { children: React.ReactNode }): React.ReactElement {
  const ref = useRef<HTMLDivElement>(null);
  const handoff = useFocusHandoff(() => ref.current?.closest<HTMLElement>("[role=alertdialog]")?.focus());
  return <div ref={ref} className="contents" {...handoff}>{children}</div>;
}
