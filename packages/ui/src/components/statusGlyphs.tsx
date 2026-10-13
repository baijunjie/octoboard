import { Hand } from "lucide-react";
import React from "react";

/** A working session's glyph: Tailwind's own ping, an accent dot sending a fading copy of itself
 * outward, still where the system asks for reduced motion. It differs from idle's speech bubble
 * by shape, so the motion is never the only cue. */
export function WorkingDot({ small }: { small?: boolean }): React.ReactElement {
  return (
    <span className={`flex shrink-0 items-center justify-center ${small ? "size-3" : "size-4"}`}>
      <span className="relative flex size-2">
        <span className="absolute inline-flex size-full rounded-full bg-accent-glyph opacity-75 motion-safe:animate-ping" />
        <span className="relative inline-flex size-2 rounded-full bg-accent-glyph" />
      </span>
    </span>
  );
}

/** The waving hand of a session waiting for the user. */
export function WavingHand({ className }: { className: string }): React.ReactElement {
  return <Hand className={`${className} origin-[70%_90%] text-warning-glyph motion-safe:animate-status-wave`} />;
}
