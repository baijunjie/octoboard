import { beforeEach, expect, it, vi } from "vitest";
import type { StatusItemMenuDraft } from "./statusItemMenu";
import { resolveCssColour } from "../cssColour";
import { withIcons } from "./statusItemIcons";

vi.mock("../cssColour", () => ({ resolveCssColour: vi.fn() }));
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
