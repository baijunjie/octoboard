import { describe, expect, it } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import { format } from "../i18n/catalog";
import type { Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import type { Session, SessionStatus } from "../protocol";
import { buildStatusItemMenu, MAX_PER_SECTION, withoutSpinnerGlyphs } from "./statusItemMenu";

const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;

const consoles = [consoleOf("c", "Main")];
const projects = [projectOf("p", "c", "Alpha")];

const menuOf = (sessions: Session[]) =>
  buildStatusItemMenu(t, {
    consoles,
    projects,
    sessions,
    consoleMap: new Map(consoles.map((c) => [c.id, c])),
    projectMap: new Map(projects.map((p) => [p.id, p])),
  });

const inProject = (id: string, status: SessionStatus, title = id, terminal_title?: string) =>
  sessionOf(id, "c", "p", title, status, { terminal_title });

describe("buildStatusItemMenu", () => {
  it("says so when no session is live", () => {
    expect(menuOf([inProject("a", "archived"), inProject("b", "interrupted")]).sections).toEqual([
      { heading: "No sessions running", items: [] },
    ]);
  });

  it("lists waiting, then working, then idle sessions, each under a heading with its count", () => {
    const { sections } = menuOf([inProject("i", "idle"), inProject("w", "working"), inProject("h", "waiting_user")]);
    expect(sections.map((s) => [s.heading, s.items.map((item) => item.session)])).toEqual([
      ["Waiting for you (1)", ["h"]],
      ["Working (1)", ["w"]],
      ["Awaiting instructions (1)", ["i"]],
    ]);
  });

  it("names a session with its location unless it is titled after it", () => {
    const { sections } = menuOf([inProject("a", "idle", "Fix bug"), inProject("b", "idle", "Alpha")]);
    expect(sections[0].items.map((item) => item.label).sort()).toEqual(["Alpha", `${isolate("Fix bug")} — ${isolate("Alpha")}`]);
  });

  it("names the first ten of a longer section and sums up the rest in a line that cannot be chosen", () => {
    const many = Array.from({ length: MAX_PER_SECTION + 3 }, (_, i) => inProject(`s${i}`, "working"));
    const [section] = menuOf(many).sections;
    expect(section.heading).toBe(`Working (${many.length})`);
    expect(section.items).toHaveLength(MAX_PER_SECTION + 1);
    expect(section.items.at(-1)).toEqual({ label: "3 more…" });
  });

  it("reads the same while an agent animates the glyph in front of its terminal title", () => {
    const frames = ["◐", "◓", "◑", "◒", "⠂", "⠐", "⠋", "✳", "✶", "✻"].map((glyph) =>
      menuOf([inProject("a", "working", "Fix bug", `${glyph} Refactor parser`)]),
    );
    for (const menu of frames) expect(menu).toEqual(frames[0]);
    expect(frames[0].sections[0].items[0].label).toContain("Refactor parser");
  });
});

describe("withoutSpinnerGlyphs", () => {
  it("removes the leading glyphs and the space after them", () => {
    expect(withoutSpinnerGlyphs("◐ Topbar")).toBe("Topbar");
    expect(withoutSpinnerGlyphs("⠙⠹  Build")).toBe("Build");
    expect(withoutSpinnerGlyphs("✦ Working…")).toBe("Working…");
    expect(withoutSpinnerGlyphs("✳")).toBe("");
  });

  it("keeps a glyph the title goes straight on from, and glyphs further in", () => {
    expect(withoutSpinnerGlyphs("●Recording")).toBe("●Recording");
    expect(withoutSpinnerGlyphs("Build ◐ done")).toBe("Build ◐ done");
    expect(withoutSpinnerGlyphs("- list")).toBe("- list");
  });
});
