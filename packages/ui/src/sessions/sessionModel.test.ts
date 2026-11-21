import { describe, expect, it } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../fixtures/builders";
import { format } from "../i18n/catalog";
import type { Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { providerModelDisplay } from "../agents/providers";
import { TEST_PROVIDERS } from "../test/testProviders";
import type { AgentModel } from "../protocol";
import { inheritedLaunch, modelDisplayName, runningLaunch, sessionModelEffort, sessionModelValues, sessionModelLabel } from "./sessionModel";

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
  const inherited = { compatibility: {}, model: "claude-sonnet-5-5", effort: "medium" };

  it("prefers what was observed, then the override, then what is inherited", () => {
    const base = sessionOf("s", "c", "p", "T", "idle");
    expect(sessionModelEffort({ ...base, observed_model: "claude-opus-5-5", observed_effort: "high", model: "x", effort: "y" }, inherited, display("claude"))).toEqual({ compatibility: {}, model: "claude-opus-5-5", effort: "high" });
    expect(sessionModelEffort({ ...base, model: "opus", effort: "low" }, inherited, display("claude"))).toEqual({ compatibility: {}, model: "opus", effort: "low" });
    expect(sessionModelEffort(base, inherited, display("claude"))).toEqual(inherited);
    // The retired alias launches as no model, so it is not shown as one.
    expect(sessionModelEffort({ ...base, model: "best" }, inherited, display("claude")).model).toBeNull();
  });

  it("takes a null override as the agent's default rather than inheriting", () => {
    expect(sessionModelEffort({ ...sessionOf("s", "c", "p", "T", "idle"), model: null, effort: null }, inherited, display("claude"))).toEqual({ compatibility: {}, model: null, effort: null });
  });

  it("takes the agent's default level where the records leave the effort unset, before any launch value", () => {
    const base = { ...sessionOf("s", "c", "p", "T", "idle"), observed_model: "gpt-6.1-sol", default_effort: "xhigh" };
    expect(sessionModelEffort({ ...base, effort: "low" }, inherited, display("claude"))).toEqual({ compatibility: {}, model: "gpt-6.1-sol", effort: "xhigh" });
    expect(sessionModelEffort(base, inherited, display("claude")).effort).toBe("xhigh");
    expect(sessionModelEffort({ ...base, observed_effort: "high" }, inherited, display("claude")).effort).toBe("high");
  });
});

describe("inheritedLaunch", () => {
  it("reads a project session from its project, then the console, and a console session from the console", () => {
    const owner = consoleOf("c", "Console", { project_session_launch: { claude: { compatibility: {}, model: "sonnet", effort: "low" } }, launch: { claude: { compatibility: {}, model: "opus", effort: "high" } } });
    const project = projectOf("p", "c", "Project", { launch: { claude: { model: "haiku" } } });
    const consoles = new Map([["c", owner]]);
    const projects = new Map([["p", project]]);
    expect(inheritedLaunch(sessionOf("s", "c", "p", "T", "idle"), consoles, projects, TEST_PROVIDERS)).toEqual({ compatibility: {}, model: "haiku", effort: "low" });
    expect(inheritedLaunch(sessionOf("s", "c", undefined, "T", "idle"), consoles, projects, TEST_PROVIDERS)).toEqual({ compatibility: {}, model: "opus", effort: "high" });
    expect(inheritedLaunch(sessionOf("s", "gone", "p", "T", "idle"), consoles, projects, TEST_PROVIDERS)).toEqual({ compatibility: {}, model: null, effort: null });
  });

  it("gives a project session whose project is not in the store the console's project session pair", () => {
    const owner = consoleOf("c", "Console", { project_session_launch: { claude: { compatibility: {}, model: "sonnet", effort: "low" } } });
    expect(inheritedLaunch(sessionOf("s", "c", "p", "T", "idle"), new Map([["c", owner]]), new Map(), TEST_PROVIDERS)).toEqual({ compatibility: {}, model: "sonnet", effort: "low" });
  });
});

describe("sessionModelLabel", () => {
  it("joins the model's short name and the effort", () => {
    expect(sessionModelLabel(t, display("claude"), { compatibility: {}, model: "claude-opus-5-5", effort: "xhigh" })).toBe(`${isolate("Opus 5.5")} · ${isolate("xhigh")}`);
  });

  it("names an unknown item as the default, and an entirely unknown pair once", () => {
    expect(sessionModelLabel(t, display("grok"), { compatibility: {}, model: "grok-4.7", effort: null })).toBe(`${isolate("grok-4.7")} · Default`);
    expect(sessionModelLabel(t, display("claude"), { compatibility: {}, model: null, effort: "high" })).toBe(`Default · ${isolate("high")}`);
    expect(sessionModelLabel(t, display("claude"), { compatibility: {}, model: null, effort: null })).toBe("Default model");
  });
});

describe("sessionModelValues", () => {
  it("gives the model's short name and the effort as separate values", () => {
    expect(sessionModelValues(t, display("claude"), { compatibility: {}, model: "claude-opus-5-5", effort: "xhigh" })).toEqual({ model: "Opus 5.5", effort: "xhigh" });
  });

  it("names each unknown value as the default", () => {
    expect(sessionModelValues(t, display("grok"), { compatibility: {}, model: "grok-4.7", effort: null })).toEqual({ model: "grok-4.7", effort: "Default" });
    expect(sessionModelValues(t, display("claude"), { compatibility: {}, model: null, effort: "high" })).toEqual({ model: "Default", effort: "high" });
    expect(sessionModelValues(t, display("claude"), { compatibility: {}, model: null, effort: null })).toEqual({ model: "Default", effort: "Default" });
  });
});

describe("runningLaunch", () => {
  const models: AgentModel[] = [
    { id: "fable", name: "Fable", efforts: ["low", "max"], version: "Fable 5.1" },
    { id: "opus", name: "Opus", efforts: ["low", "high"], version: "Opus 5.5" },
    { id: "claude-haiku-5", name: "Haiku 5", efforts: ["low"] },
  ];
  const base = sessionOf("s", "c", "p", "T", "idle");
  const running = (observed: { model?: string; effort?: string }, launch: { model: string | null; effort: string | null }, list = models, defaultModel: string | null = null) =>
    runningLaunch({ ...base, ...(observed.model && { observed_model: observed.model }), ...(observed.effort && { observed_effort: observed.effort }) }, { compatibility: {}, ...launch }, list, defaultModel, display("claude"));

  it("returns the launch pair unchanged when nothing is observed", () => {
    expect(running({}, { model: "fable", effort: "high" })).toEqual({ compatibility: {}, model: "fable", effort: "high" });
    expect(running({}, { model: null, effort: null }, models, "opus")).toEqual({ compatibility: {}, model: null, effort: null });
  });

  it("takes the listed model the session runs where the launch model is a different listed one", () => {
    expect(running({ model: "claude-opus-5-5" }, { model: "fable", effort: null }).model).toBe("opus");
    expect(running({ model: "fable" }, { model: "opus", effort: null }).model).toBe("fable");
  });

  it("measures a launch that names no model by the agent's default model", () => {
    expect(running({ model: "claude-opus-5-5" }, { model: null, effort: null }, models, "fable").model).toBe("opus");
    expect(running({ model: "claude-opus-5-5" }, { model: null, effort: null }, models, "opus").model).toBeNull();
  });

  it("leaves the launch model alone where it already is the observed one", () => {
    expect(running({ model: "claude-opus-5-5" }, { model: "opus", effort: null }).model).toBe("opus");
    expect(running({ model: "opus" }, { model: "opus", effort: null }).model).toBe("opus");
  });

  it("leaves the launch model alone where the list cannot place it or the observed model", () => {
    // The launch model is not listed.
    expect(running({ model: "claude-opus-5-5" }, { model: "retired", effort: null }).model).toBe("retired");
    // The observed model matches nothing listed.
    expect(running({ model: "claude-sonnet-5-5" }, { model: "fable", effort: null }).model).toBe("fable");
    // The list is empty (still loading, or unavailable).
    expect(running({ model: "claude-opus-5-5" }, { model: "fable", effort: null }, []).model).toBe("fable");
    // A null launch model with no default model to measure it by.
    expect(running({ model: "claude-opus-5-5" }, { model: null, effort: null }).model).toBeNull();
  });

  it("leaves the launch effort where the observed one is the same", () => {
    expect(running({ effort: "high" }, { model: "opus", effort: "high" }).effort).toBe("high");
  });

  it("takes the observed effort only where it is one of the levels the shown model has", () => {
    // Opus offers low and high, so max is no level of the model shown and the launch effort stays.
    expect(running({ effort: "max" }, { model: "opus", effort: "high" }).effort).toBe("high");
    expect(running({ effort: "low" }, { model: "opus", effort: "high" }).effort).toBe("low");
    // The shown model is the observed one, not the launch one: Fable offers max but not high.
    expect(running({ model: "fable", effort: "max" }, { model: "opus", effort: "high" })).toMatchObject({ model: "fable", effort: "max" });
    expect(running({ model: "fable", effort: "high" }, { model: "opus", effort: "low" })).toMatchObject({ model: "fable", effort: "low" });
  });

  it("measures the observed effort by the agent's default model where the shown model is none", () => {
    expect(running({ effort: "max" }, { model: null, effort: "high" }, models, "fable").effort).toBe("max");
    expect(running({ effort: "max" }, { model: null, effort: "high" }, models, "opus").effort).toBe("high");
  });

  it("keeps the launch effort while the model list is empty, even though an effort is observed", () => {
    expect(running({ effort: "low" }, { model: "opus", effort: "high" }, []).effort).toBe("high");
    expect(running({ effort: "low" }, { model: null, effort: "high" }, [], "opus").effort).toBe("high");
  });

  it("keeps an unset launch effort unset whatever is observed or listed", () => {
    expect(running({ effort: "max" }, { model: "fable", effort: null }).effort).toBeNull();
    expect(running({ effort: "low" }, { model: "opus", effort: null }, []).effort).toBeNull();
  });

  it("keeps the compatibility as given", () => {
    const launch = { compatibility: { shared: true }, model: "fable", effort: "high" };
    expect(runningLaunch({ ...base, observed_model: "claude-opus-5-5", observed_effort: "low" }, launch, models, null, display("claude")))
      .toEqual({ compatibility: { shared: true }, model: "opus", effort: "low" });
  });
});
