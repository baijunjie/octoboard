import { describe, expect, it } from "vitest";

import { consoleOf, projectOf, sessionOf } from "./gallery/fixtures/builders";
import { format } from "./i18n/catalog";
import type { Translate } from "./i18n/catalog";
import { isolate } from "./i18n/isolate";
import { inheritedLaunch, modelDisplayName, sessionModelEffort, sessionModelLabel } from "./sessionModel";

const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;

describe("modelDisplayName", () => {
  it.each([
    ["claude-opus-5-5", "Opus 5.5"],
    ["claude-opus-5", "Opus 5"],
    ["claude-opus-4-5-20251101", "Opus 4.5"],
    ["opus", "Opus"],
    ["best", "Best available"],
    ["claude-unknown", "claude-unknown"],
  ])("shows the Claude model %s as %s", (id, name) => {
    expect(modelDisplayName(t, "claude", id)).toBe(name);
  });

  it("shows another agent's id as it is, even one shaped like Claude's", () => {
    expect(modelDisplayName(t, "codex", "gpt-6.1-sol")).toBe("gpt-6.1-sol");
    expect(modelDisplayName(t, "grok", "claude-opus-5-5")).toBe("claude-opus-5-5");
    expect(modelDisplayName(t, "codex", "best")).toBe("best");
  });
});

describe("sessionModelEffort", () => {
  const inherited = { model: "claude-sonnet-5-5", effort: "medium" };

  it("prefers what was observed, then the override, then what is inherited", () => {
    const base = sessionOf("s", "c", "p", "T", "idle");
    expect(sessionModelEffort({ ...base, observed_model: "claude-opus-5-5", observed_effort: "high", model: "x", effort: "y" }, inherited)).toEqual({ model: "claude-opus-5-5", effort: "high" });
    expect(sessionModelEffort({ ...base, model: "opus", effort: "low" }, inherited)).toEqual({ model: "opus", effort: "low" });
    expect(sessionModelEffort(base, inherited)).toEqual(inherited);
  });

  it("takes a null override as the agent's default rather than inheriting", () => {
    expect(sessionModelEffort({ ...sessionOf("s", "c", "p", "T", "idle"), model: null, effort: null }, inherited)).toEqual({ model: null, effort: null });
  });

  it("takes the agent's default level where the records leave the effort unset, before any launch value", () => {
    const base = { ...sessionOf("s", "c", "p", "T", "idle"), observed_model: "gpt-6.1-sol", default_effort: "xhigh" };
    expect(sessionModelEffort({ ...base, effort: "low" }, inherited)).toEqual({ model: "gpt-6.1-sol", effort: "xhigh" });
    expect(sessionModelEffort(base, inherited).effort).toBe("xhigh");
    expect(sessionModelEffort({ ...base, observed_effort: "high" }, inherited).effort).toBe("high");
  });
});

describe("inheritedLaunch", () => {
  it("reads a project session from its project, then the console, and a console session from the console", () => {
    const owner = consoleOf("c", "Console", { claude_project_session_model: "sonnet", claude_project_session_effort: "low", claude_console_session_model: "opus", claude_console_session_effort: "high" });
    const project = projectOf("p", "c", "Project", { claude_session_model: "haiku" });
    const consoles = new Map([["c", owner]]);
    const projects = new Map([["p", project]]);
    expect(inheritedLaunch(sessionOf("s", "c", "p", "T", "idle"), consoles, projects)).toEqual({ model: "haiku", effort: "low" });
    expect(inheritedLaunch(sessionOf("s", "c", undefined, "T", "idle"), consoles, projects)).toEqual({ model: "opus", effort: "high" });
    expect(inheritedLaunch(sessionOf("s", "gone", "p", "T", "idle"), consoles, projects)).toEqual({ model: null, effort: null });
  });

  it("gives a project session whose project is not in the store the console's project session pair", () => {
    const owner = consoleOf("c", "Console", { claude_project_session_model: "sonnet", claude_project_session_effort: "low" });
    expect(inheritedLaunch(sessionOf("s", "c", "p", "T", "idle"), new Map([["c", owner]]), new Map())).toEqual({ model: "sonnet", effort: "low" });
  });
});

describe("sessionModelLabel", () => {
  it("joins the model's short name and the effort", () => {
    expect(sessionModelLabel(t, "claude", { model: "claude-opus-5-5", effort: "xhigh" })).toBe(`${isolate("Opus 5.5")} · ${isolate("xhigh")}`);
  });

  it("names an unknown item as the default, and an entirely unknown pair once", () => {
    expect(sessionModelLabel(t, "grok", { model: "grok-4.7", effort: null })).toBe(`${isolate("grok-4.7")} · Default`);
    expect(sessionModelLabel(t, "claude", { model: null, effort: "high" })).toBe(`Default · ${isolate("high")}`);
    expect(sessionModelLabel(t, "claude", { model: null, effort: null })).toBe("Default model");
  });
});
