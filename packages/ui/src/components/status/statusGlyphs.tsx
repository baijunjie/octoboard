import { Hand } from "lucide-react";
import React from "react";

/** The pinging dot of the busy statuses: Tailwind's own ping, a dot or hollow ring sending a fading
 * copy of itself outward, still where the system asks for reduced motion. The colour and the form
 * progress along the status order, so neighbours look related: `working` is a filled dot in the accent
 * blue; `background` (at the prompt, work still going) the same as a hollow ring in the accent blue,
 * telling it from the dot by shape; `delegated` (at the prompt, the sessions under it working) a hollow
 * ring in idle's green with a small filled dot in its centre, which tells it from the background ring by
 * shape where colour is not seen and holds still where the ping does. Its ring is larger than the
 * others' dots, so its ping reaches less far (`ping-small`) and ends at the edge of the normal slot; it is
 * drawn at that size only, never `small`. None differs from idle's speech bubble by colour alone, so the motion is never the only cue. The ping moves only `transform` and
 * `opacity`, and its box is laid out apart from the row (`contain-layout`; not `contain-paint`, which
 * would clip the ping's copy where it grows past the glyph), so the loop never asks the browser for a
 * layout. */
export function WorkingDot({ small, kind = "working" }: { small?: boolean; kind?: "working" | "background" | "delegated" }): React.ReactElement {
  const fill = kind === "working" ? "bg-accent-soft-foreground" : kind === "background" ? "border-2 border-accent-soft-foreground" : "border border-success-soft-foreground";
  return (
    <span className={`flex shrink-0 items-center justify-center ${small ? "size-3" : "size-4"}`}>
      <span className={`relative flex contain-layout ${kind === "delegated" ? "size-3" : "size-2"}`}>
        <span className={`absolute inline-flex size-full rounded-full ${fill} opacity-75 ${kind === "delegated" ? "motion-safe:animate-ping-small" : "motion-safe:animate-ping"}`} />
        <span className={`relative inline-flex size-full items-center justify-center rounded-full ${fill}`}>
          {kind === "delegated" && <span className="size-1 rounded-full bg-success-soft-foreground" />}
        </span>
      </span>
    </span>
  );
}

/** The waving hand of a session waiting for the user. */
export function WavingHand({ className }: { className: string }): React.ReactElement {
  return <Hand className={`${className} origin-[70%_90%] text-warning-soft-foreground motion-safe:animate-status-wave`} />;
}
