import { describe, expect, it } from "vitest";

import { opencode } from "./opencode";
import { served } from "./opencode.fixture";
import { providerColour, providerMark, providerPresentation } from "./registry";

describe("the OpenCode presentation module", () => {
  it("is the registered module of its id, and draws its mark in one path with the text colour as its fill", () => {
    expect(providerPresentation("opencode")).toBe(opencode);
    expect(providerMark("opencode")).toBe(opencode.mark);
    expect(opencode.mark.fill).toBe("currentColor");
    expect(opencode.mark.d).toMatch(/^[MmLlHhVvCcSsQqTtAaZz0-9\s.,-]+$/);
  });

  it("carries no colour, so its bars draw in the text colour", () => {
    expect(providerColour("opencode")).toBeUndefined();
    expect("colour" in opencode).toBe(false);
  });

  it("names itself the same in every language, and offers its brand name as a placeholder", () => {
    expect(opencode.params).toEqual({ opencode: "OpenCode" });

    const languages = Object.entries(opencode.messages);
    expect(languages.length).toBeGreaterThan(1);
    for (const [language, messages] of languages) {
      expect(messages, language).toMatchObject({ "provider.opencode.label": "OpenCode", "provider.opencode.shortName": "OpenCode" });
    }
  });

  it("words its pending migration as the cause of a bare launch, with a remedy that brings the additions back", () => {
    expect(opencode.codes["provider.opencode.migration_required"].recovery).toBe("provider.opencode.migration_required.remedy");
    const en = opencode.messages.en;
    expect(en["provider.opencode.migration_required"]).toBe("{agent} has yet to migrate its own configuration to its new version.");
    expect(en["provider.opencode.migration_required"]).not.toMatch(/session/);
    for (const [language, messages] of Object.entries(opencode.messages)) {
      expect(messages["provider.opencode.migration_required.remedy"], language).toContain("{appName}");
    }
  });

  // No version is refused: a minor not measured only raises a standing notice, which words the version it names.
  it("declares no refusal of a version, and words the notice of an unmeasured one with the version the daemon names, without a remedy, in every language", () => {
    expect(Object.keys(opencode.codes)).not.toContain("provider.opencode.unsupported_version");
    expect(opencode.codes["provider.opencode.version_unmeasured"]).toEqual({ origin: "daemon", codeParams: ["version"], uiParams: { agent: "OpenCode" } });
    expect(Object.keys(opencode.messages.en)).not.toContain("provider.opencode.version_unmeasured.remedy");
    expect(Object.keys(opencode.messages.en)).not.toContain("provider.opencode.unsupported_version");
    for (const [language, messages] of Object.entries(opencode.messages)) {
      const text = messages["provider.opencode.version_unmeasured"];
      for (const name of ["agent", "version", "appName"]) expect(text, `${language} {${name}}`).toContain(`{${name}}`);
    }
  });

  it("words the same messages in every language, none of them empty", () => {
    const keys = Object.keys(opencode.messages.en).sort();
    expect(keys).toContain("provider.opencode.signInHint");
    for (const [language, messages] of Object.entries(opencode.messages)) {
      expect(Object.keys(messages).sort(), language).toEqual(keys);
      for (const [key, text] of Object.entries(messages)) expect(text.trim(), `${language} ${key}`).not.toBe("");
    }
  });

  it("keeps the placeholders of every message the same in every language", () => {
    const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();
    for (const [key, english] of Object.entries(opencode.messages.en)) {
      for (const [language, messages] of Object.entries(opencode.messages)) {
        expect(placeholders(messages[key as keyof typeof messages]), `${language} ${key}`).toEqual(placeholders(english));
      }
    }
  });

  it("words its sign-in hint in every language, naming the command `opencode auth login` and no account directory", () => {
    expect(opencode.references.signInHint).toEqual({ command: "opencode auth login" });
    for (const [language, messages] of Object.entries(opencode.messages)) {
      const hint = messages["provider.opencode.signInHint"];
      expect(hint, language).toContain("{command}");
      expect(hint, language).not.toContain("{dir}");
    }
  });

  it("words each choice of its permission setting, and the confirmation only of the choice its descriptor asks one for", () => {
    const setting = served.settings.find((candidate) => candidate.id === "approval_mode");
    if (setting?.kind.type !== "choice") throw new Error("the fixture declares no approval_mode choice");
    expect(setting.kind.choices).toEqual([
      { value: "ask", confirm: false },
      { value: "auto", confirm: true },
    ]);
    for (const [language, messages] of Object.entries(opencode.messages)) {
      const keys = Object.keys(messages);
      for (const name of ["heading", "helpLabel", "helpCustom", "label", "saveFailedTitle", "helpTiming", "inheritSummary"]) {
        expect(keys, `${language} ${name}`).toContain(`provider.opencode.setting.approval_mode.${name}`);
      }
      for (const choice of setting.kind.choices) {
        const prefix = `provider.opencode.setting.approval_mode.choice.${choice.value}`;
        expect(keys, `${language} ${choice.value}`).toEqual(expect.arrayContaining([prefix, `${prefix}.summary`]));
        const confirm = `provider.opencode.setting.approval_mode.confirm.${choice.value}`;
        for (const part of ["title", "consequence"]) {
          expect(keys.includes(`${confirm}.${part}`), `${language} ${choice.value} confirm ${part}`).toBe(choice.confirm);
        }
      }
    }
  });

  it("names the agent in its permission setting's label and in the consequence of the confirmation", () => {
    for (const [language, messages] of Object.entries(opencode.messages)) {
      expect(messages["provider.opencode.setting.approval_mode.label"], language).toContain("{agent}");
      expect(messages["provider.opencode.setting.approval_mode.confirm.auto.consequence"], language).toContain("{agent}");
    }
  });

  it("serves the names and the setting its module words", () => {
    expect(served.id).toBe(opencode.id);
    expect(served.label).toBe(opencode.messages.en["provider.opencode.label"]);
    expect(served.short_name).toBe(opencode.messages.en["provider.opencode.shortName"]);
    for (const setting of served.settings) {
      expect(Object.keys(opencode.messages.en), setting.id).toContain(`provider.opencode.setting.${setting.id}.heading`);
    }
  });

  it("states its names, install address, default directory, markers and capabilities in its served-info fixture", () => {
    expect(served.id).toBe("opencode");
    expect(served.label).toBe("OpenCode");
    expect(served.short_name).toBe("OpenCode");
    expect(served.install_url).toBe("https://opencode.ai");
    expect(served.default_config_dir).toBe(".local/share/opencode");
    expect(served.detection_markers).toEqual([".opencode", "opencode.json", "opencode.jsonc", "AGENTS.md"]);
    expect(served.capabilities.preallocates_session_id).toBe(false);
    expect(served.capabilities.owned_accounts).toBe(false);
    expect(served.capabilities.mcp_inventory).toBe(true);
    expect(served.settings.map((setting) => setting.id)).toEqual(["approval_mode", "claude_compatibility"]);
    expect(served.quota_windows.kinds).toEqual([]);
  });
});
