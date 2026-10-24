import { beforeEach, expect, it, vi } from "vitest";
import type { StatusItemMenuDraft } from "./statusItemMenu";
import { resolveCssColour } from "../cssColour";
import { withIcons } from "./statusItemIcons";

vi.mock("../cssColour", () => ({ resolveCssColour: vi.fn() }));
beforeEach(() => { vi.mocked(resolveCssColour).mockReset(); });

const draft: StatusItemMenuDraft = {
  sections: [], submenus: [{ title: "Quota", items: [
    { kind: "header", title: "Available", status: "available" },
    { kind: "header", title: "Used up", status: "used_up" },
    { kind: "header", title: "Unknown", status: "unknown" },
    { kind: "header", title: "Loading" },
  ] }],
  open: "Open", quit: "Quit", attention: false,
};

it.each(["light", "dark"] as const)("uses the shared semantic palette in %s native menus", async (appearance) => {
  vi.mocked(resolveCssColour).mockImplementation((expression, theme) => {
    expect(theme).toBe(appearance);
    const palette: Record<string, string> = { "var(--success-soft-foreground)": "#12ab34", "var(--danger-soft-foreground)": "#cd5678", "var(--app-muted)": "#818283" };
    return palette[expression];
  });
  const result = await withIcons(draft, appearance);
  expect(result.submenus[0].items).toEqual([
    { kind: "header", title: "Available", status: "available", status_colour: [18, 171, 52] },
    { kind: "header", title: "Used up", status: "used_up", status_colour: [205, 86, 120] },
    { kind: "header", title: "Unknown", status: "unknown", status_colour: [129, 130, 131] },
    { kind: "header", title: "Loading" },
  ]);
  expect(draft.submenus[0].items[0]).not.toHaveProperty("status_colour");
});

it("retains the readable status when the stylesheet cannot resolve a tint", async () => {
  expect((await withIcons(draft, "light")).submenus).toEqual(draft.submenus);
});
