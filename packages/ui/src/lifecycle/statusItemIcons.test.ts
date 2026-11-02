import { beforeEach, expect, it, vi } from "vitest";
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
  ] }],
  open: "Open", quit: "Quit", attention: false,
};

it.each(["light", "dark"] as const)("gives a used-up account's bars the stylesheet's grey in %s native menus", async (appearance) => {
  vi.mocked(resolveCssColour).mockImplementation((expression, theme) => {
    expect(theme).toBe(appearance);
    return expression === "var(--meter-used-up-fill)" ? "#7b7b85" : undefined;
  });
  const result = await withIcons(draft, appearance);
  expect(result.submenus[0].items).toEqual([
    draft.submenus[0].items[0],
    { ...draft.submenus[0].items[1], used_up_colour: [123, 123, 133] },
    { ...draft.submenus[0].items[2], used_up_colour: [123, 123, 133] },
    draft.submenus[0].items[3],
  ]);
  expect(draft.submenus[0].items[1]).not.toHaveProperty("used_up_colour");
});

it("sends no grey when the stylesheet cannot resolve it", async () => {
  expect((await withIcons(draft, "light")).submenus).toEqual(draft.submenus);
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
