/** Alpha ramps at the two ends of an axis. A zero-length ramp leaves that edge alone. */
export function fadeMask(
  direction: "left" | "right" | "bottom",
  start: number,
  end: number,
  paint: "content" | "edge" = "content",
): string {
  const inside = paint === "content" ? "#000" : "transparent";
  const outside = paint === "content" ? "transparent" : "#000";
  const first = start ? `${outside} 0, ${inside} ${start}px` : `${inside} 0`;
  const last = end ? `${inside} calc(100% - ${end}px), ${outside} 100%` : `${inside} 100%`;
  return `linear-gradient(to ${direction}, ${first}, ${last})`;
}
