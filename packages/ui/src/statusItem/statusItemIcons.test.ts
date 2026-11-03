import { beforeEach, expect, it, vi } from "vitest";
import type { StatusItemSubmenuDraftItem } from "../platform";
import type { StatusItemMenuDraft } from "./statusItemMenu";
import { resolveCssColour } from "../preferences/cssColour";
import type { ShownStatusKind } from "../sessions/sessionStatus";
import { BUBBLE_SHIFT, HAND_SHIFT, shiftedTransform } from "../components/status/opticalCentring";
import { statusGlyph, withIcons } from "./statusItemIcons";

vi.mock("../preferences/cssColour", () => ({ resolveCssColour: vi.fn() }));
beforeEach(() => { vi.mocked(resolveCssColour).mockReset(); });

const meter = { session: { name: "5h", detail: "", fraction: 0 }, weekly: { name: "Weekly", detail: "", fraction: 0 }, label: "", agent: "claude" } as const;
const draft: StatusItemMenuDraft = {
  sections: [], submenus: [{ title: "Quota", items: [
    { kind: "header", title: "Used up" },
    { kind: "window", name: "5h", detail: "", label: "", fraction: 0, agent: "claude", used_up: true },
    { kind: "meter", ...meter, used_up: true },
    { kind: "window", name: "5h", detail: "", label: "", fraction: 0.5, agent: "claude" },
    { kind: "window", name: "5h", detail: "", label: "", fraction: 0.5, agent: "codex" },
  ] }],
  open: "Open", quit: "Quit", attention: false,
};

/** The draft's item at `index` as the shell receives it, less the colours: without its `agent`. */
const unagented = (index: number) => {
  const { agent: _agent, ...rest } = draft.submenus[0].items[index] as Extract<StatusItemSubmenuDraftItem, { agent: unknown }>;
  return rest;
};

const GREY = "#7b7b85";
const FILLS: Record<string, string> = { "quota-meter--claude": "#c2562f", "quota-meter--codex": "#2a6df0" };
const fillsOf = (appearance: "light" | "dark") => vi.mocked(resolveCssColour).mockImplementation((expression, theme, probeClass) => {
  expect(theme).toBe(appearance);
  if (expression === "var(--meter-used-up-fill)") return GREY;
  return expression === "var(--meter-fill)" && probeClass ? FILLS[probeClass] : undefined;
});

it.each(["light", "dark"] as const)("gives a bar its agent's stylesheet colour and a used-up account's the stylesheet's grey in %s native menus", async (appearance) => {
  fillsOf(appearance);
  const result = await withIcons(draft, appearance);
  expect(result.submenus[0].items).toEqual([
    draft.submenus[0].items[0],
    { ...unagented(1), fill_colour: [194, 86, 47], used_up_colour: [123, 123, 133] },
    { ...unagented(2), fill_colour: [194, 86, 47], used_up_colour: [123, 123, 133] },
    { ...unagented(3), fill_colour: [194, 86, 47] },
    { ...unagented(4), fill_colour: [42, 109, 240] },
  ]);
  expect(result.submenus[0].items.every((item) => !("agent" in item))).toBe(true);
  expect(draft.submenus[0].items[1]).not.toHaveProperty("used_up_colour");
  expect(draft.submenus[0].items[1]).not.toHaveProperty("fill_colour");
});

it("sends only the colours the stylesheet resolves", async () => {
  const none = (await withIcons(draft, "light")).submenus[0].items;
  expect(none).toEqual(draft.submenus[0].items.map((item, index) => ("agent" in item ? unagented(index) : item)));
  vi.mocked(resolveCssColour).mockImplementation((expression) => (expression === "var(--meter-used-up-fill)" ? GREY : undefined));
  const [, used_up, , fine] = (await withIcons(draft, "light")).submenus[0].items;
  expect(used_up).toEqual({ ...unagented(1), used_up_colour: [123, 123, 133] });
  expect(fine).toEqual(unagented(3));
});

const COLOURS: Record<string, string> = {
  "var(--accent-soft-foreground)": "#0000aa",
  "var(--success-soft-foreground)": "#00aa00",
  "var(--warning-soft-foreground)": "#aa5500",
  "var(--app-muted)": "#777777",
};

const GLYPH_STATUSES: ShownStatusKind[] = ["waiting_user", "dormant", "paused", "done", "idle", "delegated", "background", "working", "starting", "switching", "interrupted"];

it("draws every status a glyph of its own in the colour the sidebar draws it in, starting and switching account sharing the arc", () => {
  vi.mocked(resolveCssColour).mockImplementation((expression) => COLOURS[expression]);
  const glyphs = GLYPH_STATUSES.map((status) => statusGlyph(status, "light"));
  expect(glyphs.every((glyph) => glyph !== undefined)).toBe(true);
  expect(new Set(glyphs).size).toBe(GLYPH_STATUSES.length - 1);
  expect(statusGlyph("starting", "light")).toBe(statusGlyph("switching", "light"));
  expect(statusGlyph("starting", "light")).toContain("#777777");
  expect(statusGlyph("interrupted", "light")).toContain("#777777");
  expect(statusGlyph("interrupted", "light")).toContain("<rect");
  expect(statusGlyph("archived", "light")).toBeUndefined();
});

it("draws no glyph for a colour the stylesheet cannot resolve", () => {
  vi.mocked(resolveCssColour).mockReturnValue(undefined);
  for (const status of GLYPH_STATUSES) expect(statusGlyph(status, "dark")).toBeUndefined();
});

it("moves the bubble and the hand by the shifts the sidebar's glyphs take, and no other glyph", () => {
  vi.mocked(resolveCssColour).mockImplementation((expression) => COLOURS[expression]);
  expect(statusGlyph("idle", "light")).toContain(`<g transform="${shiftedTransform(BUBBLE_SHIFT)}"`);
  expect(statusGlyph("waiting_user", "light")).toContain(`<g transform="${shiftedTransform(HAND_SHIFT)}"`);
  for (const status of ["starting", "switching", "dormant", "paused", "done", "interrupted"] as const) expect(statusGlyph(status, "light")).not.toContain("transform");
});
