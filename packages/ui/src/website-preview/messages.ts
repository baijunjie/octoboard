/** The messages the website preview's frames post to the website page that embeds them, and how the page reads them.
 * Imported by both sides (the page by relative path), so it imports nothing, and a message is spelled in one place. */

export const PREVIEW_MESSAGE = {
  /** The visitor last used the keyboard or a pointer in the frame. */
  interaction: "octoboard-preview-interaction",
  /** A gesture in the frame that reached the edge of what it could scroll, handed to the page to scroll. */
  scroll: "octoboard-preview-scroll",
  ready: "octoboard-preview-ready",
  error: "octoboard-preview-error",
} as const;

export const FEATURE_MESSAGE = {
  ready: "octoboard-feature-ready",
  error: "octoboard-feature-error",
} as const;

export type Modality = "keyboard" | "pointer";

export type PreviewMessage =
  | { type: typeof PREVIEW_MESSAGE.interaction; modality: Modality }
  | { type: typeof PREVIEW_MESSAGE.scroll; deltaY: number; velocity?: number; stop?: boolean }
  | { type: typeof PREVIEW_MESSAGE.ready }
  | { type: typeof PREVIEW_MESSAGE.error };

export type FeatureMessage =
  | { type: typeof FEATURE_MESSAGE.ready; feature: string }
  | { type: typeof FEATURE_MESSAGE.error; feature?: string };

/** The frames are served from the website's own origin, and the page accepts a message only from its own frame on
 * that origin; `"/"` delivers only to a page on the frame's own origin, so another site embedding a frame hears
 * nothing. */
const PAGE_ORIGIN = "/";

/** Posts `message` from a frame to the page embedding it. */
export function postPreviewMessage(parent: Window, message: PreviewMessage): void {
  parent.postMessage(message, PAGE_ORIGIN);
}

/** Posts `message` from a feature frame to the page embedding it. */
export function postFeatureMessage(parent: Window, message: FeatureMessage): void {
  parent.postMessage(message, PAGE_ORIGIN);
}

const isFinitePart = (value: unknown): value is number => typeof value === "number" && Number.isFinite(value);

/** Whether `value` is an interaction message's modality. */
function isModality(value: unknown): value is Modality {
  return value === "keyboard" || value === "pointer";
}

/** The scroll parts of a message, or `undefined` without a finite `deltaY`; `stop` is set only by `true`, and
 * `velocity` only by a finite number. */
function readScrollParts(data: { deltaY?: unknown; velocity?: unknown; stop?: unknown }): { deltaY: number; velocity?: number; stop: boolean } | undefined {
  if (!isFinitePart(data.deltaY)) return undefined;
  return { deltaY: data.deltaY, ...(isFinitePart(data.velocity) ? { velocity: data.velocity } : {}), stop: data.stop === true };
}

/** What a preview frame's message says, or `undefined` for anything else, a malformed one included. A scroll message
 * needs a finite `deltaY`; its `stop` is set only by `true`, and its `velocity` only by a finite number. */
export function readPreviewMessage(data: unknown): PreviewMessage | undefined {
  const message = data as { type?: unknown; modality?: unknown; deltaY?: unknown; velocity?: unknown; stop?: unknown } | null | undefined;
  switch (message?.type) {
    case PREVIEW_MESSAGE.interaction:
      return isModality(message.modality) ? { type: PREVIEW_MESSAGE.interaction, modality: message.modality } : undefined;
    case PREVIEW_MESSAGE.scroll: {
      const parts = readScrollParts(message);
      return parts && { type: PREVIEW_MESSAGE.scroll, ...parts };
    }
    case PREVIEW_MESSAGE.ready:
      return { type: PREVIEW_MESSAGE.ready };
    case PREVIEW_MESSAGE.error:
      return { type: PREVIEW_MESSAGE.error };
    default:
      return undefined;
  }
}

/** What a feature frame's message says, or `undefined` for anything else. A ready message must name its feature, so
 * that the page can tell it is its own. */
export function readFeatureMessage(data: unknown): FeatureMessage | undefined {
  const message = data as { type?: unknown; feature?: unknown } | null | undefined;
  const feature = typeof message?.feature === "string" ? message.feature : undefined;
  if (message?.type === FEATURE_MESSAGE.ready) return feature === undefined ? undefined : { type: FEATURE_MESSAGE.ready, feature };
  if (message?.type === FEATURE_MESSAGE.error) return { type: FEATURE_MESSAGE.error, ...(feature === undefined ? {} : { feature }) };
  return undefined;
}
