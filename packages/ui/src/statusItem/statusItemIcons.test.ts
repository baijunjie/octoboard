import { beforeEach, expect, it, vi } from "vitest";
import type { StatusItemSubmenuDraftItem } from "../platform";
import type { StatusItemMenuDraft } from "./statusItemMenu";
import { resolveCssColour } from "../preferences/cssColour";
import { SUMMARY_STATUSES } from "../sessions/sessionStatus";
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
    { kind: "window", name: "5h", detail: "", label: "", fraction: 0.5, agent: "grok" },
  ] }],
  open: "Open", quit: "Quit", disconnect: "Disconnect", attention: false, keep_awake_label: "Awake",
  app: { this_computer: "This Mac", open_in_browser: "Browser" },
};

/** The draft's item at `index` as the shell receives it, less the colours: without its `agent`. */
const unagented = (index: number) => {
  const { agent: _agent, ...rest } = draft.submenus[0].items[index] as Extract<StatusItemSubmenuDraftItem, { agent: unknown }>;
  return rest;
};

const GREY = "#7b7b85";
const FOREGROUND = "#101010";
const fillsOf = (appearance: "light" | "dark") => vi.mocked(resolveCssColour).mockImplementation((expression, theme) => {
  expect(theme).toBe(appearance);
  if (expression === "var(--meter-used-up-fill)") return GREY;
  return expression === "var(--default-foreground)" ? FOREGROUND : undefined;
});

const FILL = {
  light: { claude: [194, 86, 47], codex: [42, 109, 240] },
  dark: { claude: [234, 131, 93], codex: [110, 160, 255] },
};

it.each(["light", "dark"] as const)("gives a bar its provider's colour, the foreground to one without, and a used-up account's the stylesheet's grey in %s native menus", async (appearance) => {
  fillsOf(appearance);
  const result = await withIcons(draft, appearance);
  expect(result.submenus[0].items).toEqual([
    draft.submenus[0].items[0],
    { ...unagented(1), fill_colour: FILL[appearance].claude, used_up_colour: [123, 123, 133] },
    { ...unagented(2), fill_colour: FILL[appearance].claude, used_up_colour: [123, 123, 133] },
    { ...unagented(3), fill_colour: FILL[appearance].claude },
    { ...unagented(4), fill_colour: FILL[appearance].codex },
    { ...unagented(5), fill_colour: [16, 16, 16] },
  ]);
  expect(result.submenus[0].items.every((item) => !("agent" in item))).toBe(true);
  expect(draft.submenus[0].items[1]).not.toHaveProperty("used_up_colour");
  expect(draft.submenus[0].items[1]).not.toHaveProperty("fill_colour");
});

it("sends only the colours the stylesheet resolves for the grey and the foreground", async () => {
  const none = (await withIcons(draft, "light")).submenus[0].items;
  expect(none).toEqual(draft.submenus[0].items.map((item, index) => (!("agent" in item) ? item : index === 5 ? unagented(index) : { ...unagented(index), fill_colour: index === 4 ? FILL.light.codex : FILL.light.claude })));
  vi.mocked(resolveCssColour).mockImplementation((expression) => (expression === "var(--meter-used-up-fill)" ? GREY : undefined));
  const [, used_up, , fine] = (await withIcons(draft, "light")).submenus[0].items;
  expect(used_up).toEqual({ ...unagented(1), fill_colour: FILL.light.claude, used_up_colour: [123, 123, 133] });
  expect(fine).toEqual({ ...unagented(3), fill_colour: FILL.light.claude });
});

const COLOURS: Record<string, string> = {
  "var(--accent-soft-foreground)": "#0000aa",
  "var(--success-soft-foreground)": "#00aa00",
  "var(--warning-soft-foreground)": "#aa5500",
  "var(--app-muted)": "#777777",
};

it("draws each status the menu lists a glyph of its own in the colour the sidebar draws it in", () => {
  vi.mocked(resolveCssColour).mockImplementation((expression) => COLOURS[expression]);
  const glyphs = SUMMARY_STATUSES.map((status) => statusGlyph(status, "light"));
  expect(glyphs.every((glyph) => glyph !== undefined)).toBe(true);
  expect(new Set(glyphs).size).toBe(SUMMARY_STATUSES.length);
  expect(statusGlyph("waiting_user", "light")).toContain("#aa5500");
  for (const status of ["paused", "done", "idle"] as const) expect(statusGlyph(status, "light")).toContain("#00aa00");
});

it("draws no glyph for a colour the stylesheet cannot resolve", () => {
  vi.mocked(resolveCssColour).mockReturnValue(undefined);
  for (const status of SUMMARY_STATUSES) expect(statusGlyph(status, "dark")).toBeUndefined();
});

it("moves the bubble and the hand by the shifts the sidebar's glyphs take, and no other glyph", () => {
  vi.mocked(resolveCssColour).mockImplementation((expression) => COLOURS[expression]);
  expect(statusGlyph("idle", "light")).toContain(`<g transform="${shiftedTransform(BUBBLE_SHIFT)}"`);
  expect(statusGlyph("waiting_user", "light")).toContain(`<g transform="${shiftedTransform(HAND_SHIFT)}"`);
  for (const status of ["paused", "done"] as const) expect(statusGlyph(status, "light")).not.toContain("transform");
});

it("leaves out the icon of a status key that names no status the menu lists, as it does one whose colour cannot be resolved", async () => {
  vi.mocked(resolveCssColour).mockImplementation((expression) => COLOURS[expression]);
  const keys = ["status:starting", "status:switching", "status:reconnecting", "status:working", "status:dormant", "status:interrupted", "status:archived", "status:unknown", "status:nonsense"];
  const menu = await withIcons({
    ...draft,
    submenus: [],
    sections: [{ heading: "Sessions", icon: keys[0], items: keys.map((icon) => ({ label: icon, icon })) }],
  }, "light");
  for (const key of keys) expect(menu.icons).not.toHaveProperty(key);
  expect(menu.icons).toEqual({});
});

it.each(["light", "dark"] as const)("uses the shared warning badge palette with alpha for the native exhausted quota tag in %s", async (appearance) => {
  vi.mocked(resolveCssColour).mockImplementation((expression, theme, alpha) => {
    expect(theme).toBe(appearance);
    if (expression === "var(--tint-warning-fg)") return "#775500";
    if (expression === "var(--tint-warning-bg)") {
      expect(alpha).toBe(true);
      return "#ffbb0026";
    }
    return undefined;
  });
  const headers = [{ kind: "header" as const, title: "Work", status: { label: "Limit reached" } }, { kind: "header" as const, title: "Home" }];
  const menu = await withIcons({ ...draft, submenus: [{ title: "Usage", items: headers }] }, appearance);
  expect(menu.submenus[0].items).toEqual([
    { ...headers[0], status: { label: "Limit reached", foreground_colour: [119, 85, 0], background_colour: [255, 187, 0, 38] } },
    headers[1],
  ]);
  expect(headers[0].status).not.toHaveProperty("foreground_colour");
});
