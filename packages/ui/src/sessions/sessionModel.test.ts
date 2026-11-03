import { describe, expect, it } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../fixtures/builders";
import { format } from "../i18n/catalog";
import type { Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { providerModelDisplay } from "../agents/providers";
import { TEST_PROVIDERS } from "../test/testProviders";
import { inheritedLaunch, modelDisplayName, sessionModelEffort, sessionModelValues, sessionModelLabel } from "./sessionModel";

const display = (agent: "claude" | "codex" | "grok") => providerModelDisplay(TEST_PROVIDERS, agent);
const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;

describe("modelDisplayName", () => {
  it.each([
    ["claude-opus-5-5", "Opus 5.5"],
    ["claude-opus-5", "Opus 5"],
    ["claude-opus-4-5-20251101", "Opus 4.5"],
    ["opus", "Opus"],
    ["fable", "Fable"],
    ["best", "best"],
    ["claude-unknown", "claude-unknown"],
    ["opus[1m]", "Opus [1m]"],
    ["claude-opus-4-5-20251101[1m]", "Opus 4.5 [1m]"],
    ["mystery[1m]", "mystery[1m]"],
  ])("shows the Claude model %s as %s", (id, name) => {
    expect(modelDisplayName(id, display("claude"))).toBe(name);
  });

  it("shows another agent's id as it is, even one shaped like Claude's", () => {
    expect(modelDisplayName("gpt-6.1-sol", display("codex"))).toBe("gpt-6.1-sol");
    expect(modelDisplayName("claude-opus-5-5", display("grok"))).toBe("claude-opus-5-5");
    expect(modelDisplayName("best", display("codex"))).toBe("best");
  });
});

describe("sessionModelEffort", () => {
  const inherited = { model: "claude-sonnet-5-5", effort: "medium" };

  it("prefers what was observed, then the override, then what is inherited", () => {
    const base = sessionOf("s", "c", "p", "T", "idle");
    expect(sessionModelEffort({ ...base, observed_model: "claude-opus-5-5", observed_effort: "high", model: "x", effort: "y" }, inherited, display("claude"))).toEqual({ model: "claude-opus-5-5", effort: "high" });
    expect(sessionModelEffort({ ...base, model: "opus", effort: "low" }, inherited, display("claude"))).toEqual({ model: "opus", effort: "low" });
    expect(sessionModelEffort(base, inherited, display("claude"))).toEqual(inherited);
    // The retired alias launches as no model, so it is not shown as one.
    expect(sessionModelEffort({ ...base, model: "best" }, inherited, display("claude")).model).toBeNull();
  });

  it("takes a null override as the agent's default rather than inheriting", () => {
    expect(sessionModelEffort({ ...sessionOf("s", "c", "p", "T", "idle"), model: null, effort: null }, inherited, display("claude"))).toEqual({ model: null, effort: null });
  });

  it("takes the agent's default level where the records leave the effort unset, before any launch value", () => {
    const base = { ...sessionOf("s", "c", "p", "T", "idle"), observed_model: "gpt-6.1-sol", default_effort: "xhigh" };
    expect(sessionModelEffort({ ...base, effort: "low" }, inherited, display("claude"))).toEqual({ model: "gpt-6.1-sol", effort: "xhigh" });
    expect(sessionModelEffort(base, inherited, display("claude")).effort).toBe("xhigh");
    expect(sessionModelEffort({ ...base, observed_effort: "high" }, inherited, display("claude")).effort).toBe("high");
  });
});

describe("inheritedLaunch", () => {
  it("reads a project session from its project, then the console, and a console session from the console", () => {
    const owner = consoleOf("c", "Console", { project_session_launch: { claude: { model: "sonnet", effort: "low" } }, launch: { claude: { model: "opus", effort: "high" } } });
    const project = projectOf("p", "c", "Project", { launch: { claude: { model: "haiku" } } });
    const consoles = new Map([["c", owner]]);
    const projects = new Map([["p", project]]);
    expect(inheritedLaunch(sessionOf("s", "c", "p", "T", "idle"), consoles, projects, TEST_PROVIDERS)).toEqual({ model: "haiku", effort: "low" });
    expect(inheritedLaunch(sessionOf("s", "c", undefined, "T", "idle"), consoles, projects, TEST_PROVIDERS)).toEqual({ model: "opus", effort: "high" });
    expect(inheritedLaunch(sessionOf("s", "gone", "p", "T", "idle"), consoles, projects, TEST_PROVIDERS)).toEqual({ model: null, effort: null });
  });

  it("gives a project session whose project is not in the store the console's project session pair", () => {
    const owner = consoleOf("c", "Console", { project_session_launch: { claude: { model: "sonnet", effort: "low" } } });
    expect(inheritedLaunch(sessionOf("s", "c", "p", "T", "idle"), new Map([["c", owner]]), new Map(), TEST_PROVIDERS)).toEqual({ model: "sonnet", effort: "low" });
  });
});

describe("sessionModelLabel", () => {
  it("joins the model's short name and the effort", () => {
    expect(sessionModelLabel(t, display("claude"), { model: "claude-opus-5-5", effort: "xhigh" })).toBe(`${isolate("Opus 5.5")} · ${isolate("xhigh")}`);
  });

  it("names an unknown item as the default, and an entirely unknown pair once", () => {
    expect(sessionModelLabel(t, display("grok"), { model: "grok-4.7", effort: null })).toBe(`${isolate("grok-4.7")} · Default`);
    expect(sessionModelLabel(t, display("claude"), { model: null, effort: "high" })).toBe(`Default · ${isolate("high")}`);
    expect(sessionModelLabel(t, display("claude"), { model: null, effort: null })).toBe("Default model");
  });
});

describe("sessionModelValues", () => {
  it("gives the model's short name and the effort as separate values", () => {
    expect(sessionModelValues(t, display("claude"), { model: "claude-opus-5-5", effort: "xhigh" })).toEqual({ model: "Opus 5.5", effort: "xhigh" });
  });

  it("names each unknown value as the default", () => {
    expect(sessionModelValues(t, display("grok"), { model: "grok-4.7", effort: null })).toEqual({ model: "grok-4.7", effort: "Default" });
    expect(sessionModelValues(t, display("claude"), { model: null, effort: "high" })).toEqual({ model: "Default", effort: "high" });
    expect(sessionModelValues(t, display("claude"), { model: null, effort: null })).toEqual({ model: "Default", effort: "Default" });
  });
});
