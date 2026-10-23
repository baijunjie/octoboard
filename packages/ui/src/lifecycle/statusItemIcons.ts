import { AGENT_MARKS } from "../components/agentMarks";
import { resolveCssColour, type Appearance } from "../cssColour";
import type { StatusItemIcon, StatusItemMenu } from "../platform";
import type { Agent } from "../protocol";
import type { StatusKind } from "../sessionLabel";
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

/** lucide's `Hand` and `MessageCircleMore`, the glyphs `StatusIcon` draws for the statuses that are
 * not a dot. */
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

function stroked(paths: string[], colour: string): string {
  const body = paths.map((d) => `<path d="${d}"/>`).join("");
  return `<g fill="none" stroke="${colour}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${body}</g>`;
}

/** The glyph colours are the ones the session list draws these statuses in (`StatusIcon`): HeroUI's soft
 * foreground of the status's colour, read from the stylesheet in the system's appearance rather than
 * written down here. */
function statusGlyph(status: StatusKind, appearance: Appearance): string | undefined {
  const colour = (token: string) => resolveCssColour(`var(--${token}-soft-foreground)`, appearance);
  const accent = colour("accent");
  const success = colour("success");
  const warning = colour("warning");
  switch (status) {
    // The accent dot of `WorkingDot`, without the ping that cannot be drawn here.
    case "working":
      return accent && `<circle cx="12" cy="12" r="5" fill="${accent}"/>`;
    // `WorkingDot`'s background ring: the same dot, hollow, in idle's green.
    case "background":
      return success && `<circle cx="12" cy="12" r="3.75" fill="none" stroke="${success}" stroke-width="2.5"/>`;
    case "waiting_user":
      return warning && stroked(HAND, warning);
    case "idle":
      return success && stroked(BUBBLE, success);
    default:
      return undefined;
  }
}

/** The glyph a key stands for as SVG content, and whether it is a template. */
function glyphOf(key: string, appearance: Appearance): { svg: string; template: boolean } | undefined {
  const [kind, name] = key.split(":");
  if (kind === "agent") {
    const mark = AGENT_MARKS[name as Agent];
    if (!mark) return undefined;
    const template = mark.fill === "currentColor";
    return { svg: `<path d="${mark.d}" fill="${template ? "#000" : mark.fill}" fill-rule="evenodd"/>`, template };
  }
  const svg = kind === "status" ? statusGlyph(name as StatusKind, appearance) : undefined;
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
 * its line is shown without a picture. */
export async function withIcons(draft: StatusItemMenuDraft, appearance: Appearance): Promise<StatusItemMenu> {
  const keys = iconKeysOf(draft);
  const icons: StatusItemMenu["icons"] = {};
  await Promise.all(
    [...keys].map(async (key) => {
      const icon = key === undefined ? undefined : await iconOf(key, appearance);
      if (key !== undefined && icon) icons[key] = icon;
    }),
  );
  return { ...draft, icons };
}
