import { AGENT_MARKS } from "../components/agentMarks";
import type { StatusItemIcon, StatusItemMenu } from "../platform";
import type { Agent, SessionStatus } from "../protocol";
import type { StatusItemMenuDraft } from "./statusItemMenu";

/** What the shell's menu shows beside a line: the mark of a session's agent, and a heading's status
 * glyph. A menu cannot animate, so each is drawn once as a still picture. A mark the UI draws in the
 * text colour (Codex, Grok) is a template image, drawn in black for its alpha alone, which the
 * system tints to suit a light or dark menu; the others keep their own colours, chosen to read on
 * both. */

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

const STATUS_GLYPHS: Partial<Record<SessionStatus, string>> = {
  // The accent dot of `WorkingDot`, without the ping that cannot be drawn here.
  working: "<circle cx=\"12\" cy=\"12\" r=\"5\" fill=\"#3b82f6\"/>",
  waiting_user: stroked(HAND, "#d98200"),
  idle: stroked(BUBBLE, "#22a05a"),
};

/** The glyph a key stands for as SVG content, and whether it is a template. */
function glyphOf(key: string): { svg: string; template: boolean } | undefined {
  const [kind, name] = key.split(":");
  if (kind === "agent") {
    const mark = AGENT_MARKS[name as Agent];
    if (!mark) return undefined;
    const template = mark.fill === "currentColor";
    return { svg: `<path d="${mark.d}" fill="${template ? "#000" : mark.fill}" fill-rule="evenodd"/>`, template };
  }
  const svg = kind === "status" ? STATUS_GLYPHS[name as SessionStatus] : undefined;
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

async function iconOf(key: string): Promise<StatusItemIcon | undefined> {
  const glyph = glyphOf(key);
  if (!glyph) return undefined;
  let png = drawn.get(key);
  if (!png) {
    png = rasterize(glyph.svg);
    drawn.set(key, png);
  }
  try {
    return { png: await png, template: glyph.template };
  } catch {
    drawn.delete(key);
    return undefined;
  }
}

/** The menu with the pictures its lines name, each key drawn once for the life of the page. A key
 * that cannot be drawn is left out, and its line is shown without a picture. */
export async function withIcons(draft: StatusItemMenuDraft): Promise<StatusItemMenu> {
  const keys = new Set(draft.sections.flatMap((section) => [section.icon, ...section.items.map((item) => item.icon)]));
  const icons: StatusItemMenu["icons"] = {};
  await Promise.all(
    [...keys].map(async (key) => {
      const icon = key === undefined ? undefined : await iconOf(key);
      if (key !== undefined && icon) icons[key] = icon;
    }),
  );
  return { ...draft, icons };
}
