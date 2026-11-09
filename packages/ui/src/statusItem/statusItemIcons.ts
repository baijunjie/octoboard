import { DELEGATED_DOT_PX, DELEGATED_RING_PX } from "../components/status/delegatedGlyph";
import { BUBBLE_SHIFT, HAND_SHIFT, shiftedTransform, type OpticalShift } from "../components/status/opticalCentring";
import { SLEEP_PATHS } from "../components/status/sleepGlyph";
import { resolveCssColour, type Appearance } from "../preferences/cssColour";
import { providerColour, providerMark } from "../providers/registry";
import type { StatusItemIcon, StatusItemMenu, StatusItemSubmenuDraftItem, StatusItemSubmenuItem } from "../platform";
import type { ShownStatusKind } from "../sessions/sessionStatus";
import type { StatusItemMenuDraft } from "./statusItemMenu";

/** What the shell's menu shows beside a line: the mark of a session's agent, and a heading's status
 * glyph. A menu cannot animate, so each is drawn as a still picture. A mark the UI draws in the
 * text colour (Codex, Grok) is a template image, drawn in black for its alpha alone, which the
 * system tints to suit a light or dark menu; the others keep their own colours, a status glyph's those
 * of the appearance the app sets, which the menu follows (the app's `NSAppearance` is set from the user's
 * theme, `tauriNativeWindow().setTheme`, and the page's `prefers-color-scheme` follows it too). */

/** The picture's size in the menu, and how much finer it is drawn so it stays sharp on a Retina
 * display. */
const SIZE = 18;
const SCALE = 2;

/** lucide's `Hand`, `MessageCircleMore`, `CircleCheck`, `CirclePause`, `LoaderCircle` and `Pause`, and the
 * dormant "zz" (`SLEEP_PATHS`), the glyphs `StatusIcon` draws for the statuses that are not a dot.
 * `CircleCheck`, `CirclePause` and `Pause` have shapes that are not all paths, so they are given as markup. */
const HAND = [
  "M18 11V6a2 2 0 0 0-2-2a2 2 0 0 0-2 2",
  "M14 10V4a2 2 0 0 0-2-2a2 2 0 0 0-2 2v2",
  "M10 10.5V6a2 2 0 0 0-2-2a2 2 0 0 0-2 2v8",
  "M18 8a2 2 0 1 1 4 0v6a8 8 0 0 1-8 8h-2c-2.8 0-4.5-.86-5.99-2.34l-3.6-3.6a2 2 0 0 1 2.83-2.82L7 15",
];
const BUBBLE = [
  "M2.992 16.342a2 2 0 0 1 .094 1.167l-1.065 3.29a1 1 0 0 0 1.236 1.168l3.413-.998a2 2 0 0 1 1.099.092 10 10 0 1 0-4.777-4.719",
  "M8 12h.01",
  "M12 12h.01",
  "M16 12h.01",
];

const PAUSED = "<circle cx=\"12\" cy=\"12\" r=\"10\"/><line x1=\"10\" x2=\"10\" y1=\"15\" y2=\"9\"/><line x1=\"14\" x2=\"14\" y1=\"15\" y2=\"9\"/>";

const ARC = ["M21 12a9 9 0 1 1-6.219-8.56"];

const PAUSE = "<rect x=\"14\" y=\"3\" width=\"5\" height=\"18\" rx=\"1\"/><rect x=\"5\" y=\"3\" width=\"5\" height=\"18\" rx=\"1\"/>";

const CHECK = "<circle cx=\"12\" cy=\"12\" r=\"10\"/><path d=\"m16 9-5.5 5.5L8 12\"/>";

/** The glyph's strokes, moved by `shift` where the glyph is one whose ink is lopsided (the same shift `StatusIcon` gives it). */
function stroked(paths: string[] | string, colour: string, shift?: OpticalShift): string {
  const body = typeof paths === "string" ? paths : paths.map((d) => `<path d="${d}"/>`).join("");
  const moved = shift ? ` transform="${shiftedTransform(shift)}"` : "";
  return `<g${moved} fill="none" stroke="${colour}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${body}</g>`;
}

/** The glyph's units per CSS pixel of the sidebar's glyphs: the working dot's 8px is drawn with r="5". */
const MENU_UNITS_PER_PX = 1.25;

/** The glyph colours are the ones the session list draws these statuses in (`StatusIcon`): HeroUI's soft
 * foreground of the status's colour, read from the stylesheet in the system's appearance rather than
 * written down here. Every status but archived has one; `undefined` also stands for a colour the
 * stylesheet cannot resolve. */
export function statusGlyph(status: ShownStatusKind, appearance: Appearance): string | undefined {
  const colour = (token: string) => resolveCssColour(`var(--${token}-soft-foreground)`, appearance);
  const accent = colour("accent");
  const success = colour("success");
  const warning = colour("warning");
  // `StatusIcon`'s `text-app-muted`, the ink of the arc and of interrupted's pause sign.
  const muted = resolveCssColour("var(--app-muted)", appearance);
  switch (status) {
    // The accent dot of `WorkingDot`, without the ping that cannot be drawn here.
    case "working":
      return accent && `<circle cx="12" cy="12" r="5" fill="${accent}"/>`;
    // `WorkingDot`'s background ring: the dot's size, hollow, in idle's green.
    case "background":
      return success && `<circle cx="12" cy="12" r="3.75" fill="none" stroke="${success}" stroke-width="2.5"/>`;
    // `WorkingDot`'s delegated glyph: a ring in idle's green around a dot, at the scale the working dot is
    // drawn here (its 8px as 10 units), the ring's line thickened to 2 units to read at menu size.
    case "delegated": {
      const outer = (DELEGATED_RING_PX * MENU_UNITS_PER_PX) / 2;
      const dot = (DELEGATED_DOT_PX * MENU_UNITS_PER_PX) / 2;
      return success && `<circle cx="12" cy="12" r="${outer - 1}" fill="none" stroke="${success}" stroke-width="2"/><circle cx="12" cy="12" r="${dot}" fill="${success}"/>`;
    }
    case "waiting_user":
      return warning && stroked(HAND, warning, HAND_SHIFT);
    case "dormant":
      return warning && stroked([...SLEEP_PATHS], warning);
    case "paused":
      return success && stroked(PAUSED, success);
    case "idle":
      return success && stroked(BUBBLE, success, BUBBLE_SHIFT);
    case "done":
      return success && stroked(CHECK, success);
    // Still, as a menu cannot turn it: switching account and reconnecting show the same arc as starting.
    case "starting":
    case "switching":
    case "reconnecting":
      return muted && stroked(ARC, muted);
    case "interrupted":
      return muted && stroked(PAUSE, muted);
    case "archived":
      return undefined;
  }
}

/** The glyph a key stands for as SVG content, and whether it is a template. */
function glyphOf(key: string, appearance: Appearance): { svg: string; template: boolean } | undefined {
  const [kind, name] = key.split(":");
  if (kind === "agent") {
    const mark = providerMark(name);
    const template = mark.fill === "currentColor";
    return { svg: `<path d="${mark.d}" fill="${template ? "#000" : mark.fill}" fill-rule="evenodd"/>`, template };
  }
  const svg = kind === "status" ? statusGlyph(name as ShownStatusKind, appearance) : undefined;
  return svg ? { svg, template: false } : undefined;
}

async function rasterize(glyph: string): Promise<string> {
  const px = SIZE * SCALE;
  // The glyph sits in the middle of the picture with a margin, as the system's own menu pictures do.
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${px}" height="${px}" viewBox="-2 -2 28 28">${glyph}</svg>`;
  const image = new Image();
  image.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
  await image.decode();
  const canvas = document.createElement("canvas");
  canvas.width = px;
  canvas.height = px;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("no 2d canvas context");
  context.drawImage(image, 0, 0, px, px);
  return canvas.toDataURL("image/png").slice("data:image/png;base64,".length);
}

const drawn = new Map<string, Promise<string>>();

async function iconOf(key: string, appearance: Appearance): Promise<StatusItemIcon | undefined> {
  const glyph = glyphOf(key, appearance);
  if (!glyph) return undefined;
  let png = drawn.get(glyph.svg);
  if (!png) {
    png = rasterize(glyph.svg);
    drawn.set(glyph.svg, png);
  }
  try {
    return { png: await png, template: glyph.template };
  } catch {
    drawn.delete(glyph.svg);
    return undefined;
  }
}

/** The keys of the pictures the menu's lines name, each once: the sessions' and headings', and the
 * submenus' marks. */
export function iconKeysOf(draft: StatusItemMenuDraft): Set<string | undefined> {
  return new Set([
    ...draft.sections.flatMap((section) => [section.icon, ...section.items.map((item) => item.icon)]),
    ...draft.submenus.map((submenu) => submenu.icon),
  ]);
}

/** The menu with the pictures its lines name, in the system's `appearance`. Each picture is drawn once for
 * as long as its colours stay the same, so a change of appearance draws the status glyphs again and the
 * pictures of the other appearance are kept for its return. A key that cannot be drawn is left out, and
 * its line is shown without a picture. The bars take their agent's colour from its presentation module (the
 * foreground for one without), and those of a used-up account the stylesheet's grey for them. */
export async function withIcons(draft: StatusItemMenuDraft, appearance: Appearance): Promise<StatusItemMenu> {
  const keys = iconKeysOf(draft);
  const icons: StatusItemMenu["icons"] = {};
  await Promise.all(
    [...keys].map(async (key) => {
      const icon = key === undefined ? undefined : await iconOf(key, appearance);
      if (key !== undefined && icon) icons[key] = icon;
    }),
  );
  const bytesOf = (colour: string | undefined): [number, number, number] | undefined =>
    colour ? ([1, 3, 5].map((offset) => Number.parseInt(colour.slice(offset, offset + 2), 16)) as [number, number, number]) : undefined;
  const used_up_colour = bytesOf(resolveCssColour("var(--meter-used-up-fill)", appearance));
  // A provider without a colour of its own draws in the foreground, as its bars do on the page.
  const neutral = bytesOf(resolveCssColour("var(--default-foreground)", appearance));
  const coloured = (item: StatusItemSubmenuDraftItem): StatusItemSubmenuItem => {
    if (item.kind === "header" && item.status) {
      const foreground_colour = bytesOf(resolveCssColour("var(--tint-warning-fg)", appearance));
      const background = resolveCssColour("var(--tint-warning-bg)", appearance, true);
      const background_colour = background ? [1, 3, 5, 7].map((offset) => Number.parseInt(background.slice(offset, offset + 2), 16)) as [number, number, number, number] : undefined;
      return { ...item, status: { ...item.status, ...(foreground_colour && { foreground_colour }), ...(background_colour && { background_colour }) } };
    }
    if (item.kind !== "window" && item.kind !== "meter") return item;
    const { agent, ...rest } = item;
    const fill_colour = bytesOf(providerColour(agent)?.[appearance]) ?? neutral;
    return { ...rest, ...(fill_colour && { fill_colour }), ...(item.used_up && used_up_colour && { used_up_colour }) };
  };
  return {
    ...draft,
    icons,
    submenus: draft.submenus.map((submenu) => ({ ...submenu, items: submenu.items.map(coloured) })),
  };
}
