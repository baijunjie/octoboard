import type { Agent } from "../protocol";
import type { ShownStatusKind } from "../sessions/sessionStatus";

/** The keys by which the menu's lines name their pictures in `StatusItemMenu.icons`; the pictures
 * themselves are drawn by `statusItemIcons.ts`. */

export function agentIconKey(agent: Agent): string {
  return `agent:${agent}`;
}

/** The key of a status heading's glyph. */
export function statusIconKey(status: ShownStatusKind): string {
  return `status:${status}`;
}
