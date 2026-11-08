import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

import { describe, expect, it } from "vitest";

import type { ModelDisplay, ProviderInfo, SettingDescriptor } from "../protocol";
import { FIXTURE_PROVIDERS } from "./fixtureProviders";

// The fixture daemon serves a hand-written copy of what the daemon's registry describes of each provider, so a
// descriptor that changes there would otherwise leave every scenario showing the old install link, window names or
// settings. This reads each provider's descriptor out of `apps/daemon/src/providers/<id>.rs` by its shape (rustfmt's
// four-space fields) and holds the copy to it; a field it cannot find fails the test rather than passing it.
const PROVIDERS = new URL("../../../../apps/daemon/src/providers/", pathToFileURL(`${import.meta.dirname}/`));

function read(file: string): string {
  return readFileSync(new URL(file, PROVIDERS), "utf8").replace(/^\s*\/\/.*$/gm, "");
}

function required(source: string, pattern: RegExp, what: string): string {
  const found = pattern.exec(source)?.[1];
  if (found === undefined) throw new Error(`The descriptor has no ${what}`);
  return found;
}

/** The text between `start` and the first `end` after it, the descriptor's field of that name. */
function between(source: string, start: string, end: string): string {
  const from = source.indexOf(start);
  const to = source.indexOf(end, from);
  if (from < 0 || to < 0) throw new Error(`The descriptor has no ${start}`);
  return source.slice(from + start.length, to);
}

const snake = (name: string) => name.replace(/(?<!^)([A-Z])/g, "_$1").toLowerCase();

function capabilitiesOf(source: string): ProviderInfo["capabilities"] {
  const body = between(source, "capabilities: Capabilities {", "quota_windows:");
  const flag = (name: string) => required(body, new RegExp(`\\b${name}: (true|false)`), name) === "true";
  return {
    preallocates_session_id: flag("preallocates_session_id"),
    hook_script: flag("hook_script"),
    status_line: flag("status_line"),
    reset_credits: flag("reset_credits"),
    per_session_trust: flag("per_session_trust"),
    config_sharing: flag("config_sharing"),
    mcp_inventory: flag("mcp_inventory"),
    delivery: {
      confirms_by_reported_prompt: flag("confirms_by_reported_prompt"),
      holds_report_for_completion_notice: flag("holds_report_for_completion_notice"),
      resume_instruction_as_launch_prompt: flag("resume_instruction_as_launch_prompt"),
    },
  };
}

function quotaWindowsOf(source: string): ProviderInfo["quota_windows"] {
  const body = between(source, "quota_windows: QuotaWindowNaming {", "model_display:");
  return {
    kinds: [...between(body, "kinds:", "session_window:").matchAll(/QuotaWindowKind::(\w+)/g)].map(([, kind]) => snake(kind)) as ProviderInfo["quota_windows"]["kinds"],
    session_window: snake(required(body, /session_window: SessionWindowName::(\w+)/, "session window")) as ProviderInfo["quota_windows"]["session_window"],
    weekly_all_models: required(body, /weekly_all_models: (true|false)/, "weekly_all_models") === "true",
  };
}

function modelDisplayOf(source: string): ModelDisplay {
  const body = between(source, "model_display:", "compatibility_source:");
  if (body.includes("ModelDisplay::PLAIN")) return { family_version: false, id_prefix: null, aliases: [], retired: [] };
  const words = (field: string) => [...required(body, new RegExp(`${field}: &\\[([^\\]]*)\\]`), field).matchAll(/"([^"]+)"/g)].map(([, word]) => word);
  const prefix = required(body, /id_prefix: (None|Some\("[^"]*"\))/, "id_prefix");
  return {
    family_version: required(body, /family_version: (true|false)/, "family_version") === "true",
    id_prefix: prefix === "None" ? null : prefix.slice(6, -2),
    aliases: words("aliases"),
    retired: words("retired"),
  };
}

function settingsOf(source: string): SettingDescriptor[] {
  const body = between(source, "settings: &[", "codes:");
  return body.split("SettingDescriptor {").slice(1).map((setting): SettingDescriptor => {
    const compatibility = required(setting, /compatibility_with: (None|Some\("[^"]*"\))/, "compatibility_with");
    const kind: SettingDescriptor["kind"] = setting.includes("SettingKind::Choice")
      ? { type: "choice", choices: [...setting.matchAll(/value: "([^"]+)",\s*confirm: (true|false)/g)].map(([, value, confirm]) => ({ value, confirm: confirm === "true" })) }
      : { type: "switch", default: switchDefault(required(setting, /default: SwitchDefault::(\w+(?:\("[^"]*"\))?)/, "default")) };
    return {
      id: required(setting, /\bid: "([a-z_]+)"/, "setting id"),
      kind,
      exported: required(setting, /exported: (true|false)/, "exported") === "true",
      shown_to_agents: required(setting, /shown_to_agents: (true|false)/, "shown_to_agents") === "true",
      compatibility_with: compatibility === "None" ? null : compatibility.slice(6, -2),
    };
  });
}

function switchDefault(rule: string): Extract<SettingDescriptor["kind"], { type: "switch" }>["default"] {
  if (rule === "On") return "on";
  if (rule === "Off") return "off";
  return { while_added: required(rule, /^WhileAdded\("([^"]*)"\)$/, "WhileAdded id") };
}

describe.each(FIXTURE_PROVIDERS.map((provider) => [provider.id, provider] as const))("the fixture provider %s", (id, served) => {
  const source = read(`${id}.rs`);

  it("serves the names, install link and configuration directory of its descriptor", () => {
    expect(served.id).toBe(required(source, /^ {4}id: "([a-z_]+)",$/m, "id"));
    expect(served.label).toBe(required(source, /^ {4}label: "([^"]+)",$/m, "label"));
    expect(served.short_name).toBe(required(source, /^ {4}short_name: "([^"]+)",$/m, "short_name"));
    expect(served.install_url).toBe(required(source, /^ {4}install_url: "([^"]+)",$/m, "install_url"));
    expect(served.default_config_dir).toBe(required(source, /const LAYOUT: AgentLayout = AgentLayout \{[\s\S]*?default_dir: "([^"]+)"/, "default_dir"));
  });

  it("serves the capabilities of its descriptor", () => {
    expect(served.capabilities).toEqual(capabilitiesOf(source));
  });

  it("serves the quota window naming and model display of its descriptor", () => {
    expect(served.quota_windows).toEqual(quotaWindowsOf(source));
    expect(served.model_display).toEqual(modelDisplayOf(source));
  });

  it("serves the settings of its descriptor", () => {
    expect(served.settings).toEqual(settingsOf(source));
  });
});

it("serves every provider the daemon registers, in its order", () => {
  const registry = read("mod.rs");
  const registered = [...between(registry, "const REGISTRY: &[&ProviderDescriptor] = &[", "];").replace(/#\[cfg\(test\)\]\s*&test_provider::DESCRIPTOR,/, "").matchAll(/&(\w+)::DESCRIPTOR/g)].map(([, module]) => module);
  expect(FIXTURE_PROVIDERS.map(({ id }) => id)).toEqual(registered);
});
