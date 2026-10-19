import { Hand } from "lucide-react";
import React from "react";

/** A working session's glyph: Tailwind's own ping, an accent dot sending a fading copy of itself
 * outward, still where the system asks for reduced motion. It differs from idle's speech bubble
 * by shape, so the motion is never the only cue. `background` draws a session running in the
 * background: the same ping as a hollow ring in idle's green, work still going though the agent
 * sits at its prompt, the ring telling it from the filled dot by shape as well as colour. The ping
 * moves only `transform` and `opacity`, and its box is laid out apart from the row
 * (`contain-layout`; not `contain-paint`, which would clip the ping's copy where it grows past the
 * glyph), so the loop never asks the browser for a layout. */
export function WorkingDot({ small, background }: { small?: boolean; background?: boolean }): React.ReactElement {
  const fill = background ? "border-2 border-success" : "bg-accent-glyph";
  return (
    <span className={`flex shrink-0 items-center justify-center ${small ? "size-3" : "size-4"}`}>
      <span className="relative flex size-2 contain-layout">
        <span className={`absolute inline-flex size-full rounded-full ${fill} opacity-75 motion-safe:animate-ping`} />
        <span className={`relative inline-flex size-2 rounded-full ${fill}`} />
      </span>
    </span>
  );
}

/** The waving hand of a session waiting for the user. */
export function WavingHand({ className }: { className: string }): React.ReactElement {
  return <Hand className={`${className} origin-[70%_90%] text-warning-glyph motion-safe:animate-status-wave`} />;
}
