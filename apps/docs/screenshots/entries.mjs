// The screenshot manifest's entries: which UI state gallery scenarios the Features pages show, and how each is captured.
// The entries' fields and their validation, which `manifest.mjs` (the capture side) and the `Screenshot` component (the
// site build) share, so this module imports nothing of the capture tooling.
//
// An entry has these fields, in English only (the language is always `en`):
//   id        The output file name, `src/assets/features/<id>.png`: lowercase words joined by hyphens, unique across
//             all groups.
//   scenario  The gallery scenario shown (an id of `packages/ui/src/gallery/fixtures/`). Scenarios are written to
//             test the UI, so many carry stress data (long right-to-left and CJK names, fixture file names) or open
//             with a focus ring showing; look at each capture before a page uses it.
//   width     The window width in pixels, default 1440. A scenario that carries a `width` of its own is captured at
//             exactly that width, and one with a `minWidth` at no less, which the script checks.
//   height    The window height in pixels, default 900. A scenario that carries a `height` of its own is captured at
//             exactly that height, which the script checks.
//   clip      Capture one element instead of the whole window: a Playwright selector matching exactly one visible
//             element, or `{ selector, padding }` to change the `padding` pixels around it (default 24). The capture
//             is the element alone over a transparent background: everything outside it is hidden, so its rounded
//             corners show nothing behind them and its own shadow fades out softly in the margin. A panel or sidebar
//             at the window's edge takes `padding: 0`, since the window has no room for a margin there.
//   keep      With a `clip`: a Playwright selector, or an array of them, naming the floating layers (a tooltip drawn in
//             its own portal, outside the clipped element) that stay visible with their contents, since the picture
//             is of them. Each matches at least one element, which the script checks, but only the capture shows
//             whether the layer is in the picture: look at it. Nothing else outside the clipped element is kept.
//   scale     The device pixel ratio the page is captured at: 2 (default, crisp on high-density screens) or 1. The
//             terminal's GPU renderer draws its glyphs at twice their size in headless Chromium at 2, so a
//             scenario showing terminal content takes 1.
//   hover     A Playwright selector matching exactly one visible element: before the capture the mouse is moved to its
//             centre, for what shows only under the pointer (a row's buttons).
// Every capture presses as a mouse does (`input=mouse`, see `frameUrl`), so no keyboard focus ring shows. A scenario's
// step that presses a control a mouse click does not open (a status mark, which opens on hover) therefore fails here, so
// a scenario a screenshot uses hovers such a control instead. Only this command loads frames that way: the gallery's
// sweep and quick check do not catch such a step.
/** The manifest modules, one per feature group. */
export const GROUPS = ["accounts", "context", "collaboration"];

export const DEFAULT_WIDTH = 1440;
export const DEFAULT_HEIGHT = 900;
export const DEFAULT_SCALE = 2;
export const DEFAULT_CLIP_PADDING = 24;
const SCALES = [1, 2];
const FIELDS = ["id", "scenario", "width", "height", "scale", "clip", "keep", "hover"];

const positiveInteger = (value) => Number.isInteger(value) && value > 0;

/** `entry` with its defaults filled in, or an Error naming what is wrong with it. `where` says which entry. */
export function normalizeEntry(entry, where) {
  const fail = (message) => {
    throw new Error(`${where}: ${message}`);
  };
  if (entry === null || typeof entry !== "object" || Array.isArray(entry)) fail("an entry is an object");
  const unknown = Object.keys(entry).filter((key) => !FIELDS.includes(key));
  if (unknown.length > 0) fail(`unknown field ${unknown.join(", ")}; the fields are ${FIELDS.join(", ")}`);
  if (typeof entry.id !== "string" || !/^[a-z0-9]+(-[a-z0-9]+)*$/.test(entry.id)) fail("`id` is the output file name: lowercase words joined by hyphens");
  if (typeof entry.scenario !== "string" || entry.scenario === "") fail("`scenario` is a gallery scenario id");
  const width = entry.width ?? DEFAULT_WIDTH;
  const height = entry.height ?? DEFAULT_HEIGHT;
  if (!positiveInteger(width)) fail("`width` is a positive whole number of pixels");
  if (!positiveInteger(height)) fail("`height` is a positive whole number of pixels");
  const scale = entry.scale ?? DEFAULT_SCALE;
  if (!SCALES.includes(scale)) fail(`\`scale\` is ${SCALES.join(" or ")}`);
  let clip;
  if (entry.clip !== undefined) {
    if (entry.clip === null || (typeof entry.clip !== "string" && (typeof entry.clip !== "object" || Array.isArray(entry.clip)))) fail("`clip` is a Playwright selector, or { selector, padding }");
    const { selector, padding = DEFAULT_CLIP_PADDING, ...rest } = typeof entry.clip === "string" ? { selector: entry.clip } : entry.clip;
    if (Object.keys(rest).length > 0) fail("`clip` takes only `selector` and `padding`");
    if (typeof selector !== "string" || selector === "") fail("`clip` is a Playwright selector, or { selector, padding }");
    if (typeof padding !== "number" || !Number.isFinite(padding) || padding < 0) fail("`clip.padding` is a number of pixels, 0 or more");
    clip = { selector, padding };
  }
  let keep;
  if (entry.keep !== undefined) {
    const list = Array.isArray(entry.keep) ? entry.keep : [entry.keep];
    if (list.length === 0 || list.some((selector) => typeof selector !== "string" || selector === "")) fail("`keep` is a Playwright selector, or an array of them");
    if (clip === undefined) fail("`keep` needs a `clip`: it names what stays visible outside the clipped element");
    keep = list;
  }
  if (entry.hover !== undefined && (typeof entry.hover !== "string" || entry.hover === "")) fail("`hover` is a Playwright selector");
  return { id: entry.id, scenario: entry.scenario, width, height, scale, clip, keep, hover: entry.hover };
}

/** Every entry of `groups`, a map from group name to the entries it lists, normalized and tagged with its group.
 * Output file names are shared by all groups, so an id used twice is refused. */
export function normalizeManifest(groups) {
  const seen = new Map();
  const entries = [];
  for (const [group, list] of Object.entries(groups)) {
    if (!Array.isArray(list)) throw new Error(`${group}: the manifest default-exports an array of entries`);
    list.forEach((raw, index) => {
      const entry = normalizeEntry(raw, `${group}[${index}]${typeof raw?.id === "string" ? ` (${raw.id})` : ""}`);
      if (seen.has(entry.id)) throw new Error(`${group}: the id ${entry.id} is also used in ${seen.get(entry.id)}`);
      seen.set(entry.id, group);
      entries.push({ ...entry, group });
    });
  }
  return entries;
}
