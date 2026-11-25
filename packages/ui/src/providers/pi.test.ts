import { describe, expect, it } from "vitest";

import { LANGUAGES } from "../i18n/languages";
import { pi } from "./pi";
import { served } from "./pi.fixture";
import { providerColour, providerMark, providerPresentation } from "./registry";

describe("the Pi presentation module", () => {
  it("is the registered module of its id, and draws its mark in one path with the text colour as its fill", () => {
    expect(providerPresentation("pi")).toBe(pi);
    expect(providerMark("pi")).toBe(pi.mark);
    expect(pi.mark.fill).toBe("currentColor");
    expect(pi.mark.d).toMatch(/^[MmLlHhVvCcSsQqTtAaZz0-9\s.,-]+$/);
  });

  it("carries no colour, so its bars draw in the text colour", () => {
    expect(providerColour("pi")).toBeUndefined();
    expect("colour" in pi).toBe(false);
  });

  it("names itself the same in every language, and offers its brand name as a placeholder", () => {
    expect(pi.params).toEqual({ pi: "Pi" });
    expect(pi.codes).toEqual({});
    const languages = Object.entries(pi.messages);
    expect(languages.length).toBeGreaterThan(1);
    for (const [language, messages] of languages) {
      expect(messages["provider.pi.label"], language).toBe("Pi");
      expect(messages["provider.pi.shortName"], language).toBe("Pi");
    }
  });

  it("words the same messages in every language Octoboard offers", () => {
    expect(Object.keys(pi.messages).sort()).toEqual([...LANGUAGES].sort());
    const keys = Object.keys(pi.messages.en).sort();
    expect(keys).toEqual(["provider.pi.label", "provider.pi.shortName", "provider.pi.signInHint"]);
    for (const [language, messages] of Object.entries(pi.messages)) {
      expect(Object.keys(messages).sort(), language).toEqual(keys);
    }
  });

  it("words its sign-in hint in every language, naming the command, the variable and the account's directory", () => {
    expect(pi.references.signInHint).toEqual({ command: "pi auth login <provider>", variable: "PI_CODING_AGENT_DIR" });
    for (const [language, messages] of Object.entries(pi.messages)) {
      const hint = messages["provider.pi.signInHint"];
      for (const placeholder of ["{command}", "{variable}", "{dir}"]) expect(hint, `${language} ${placeholder}`).toContain(placeholder);
    }
  });

  it("states its names, install address, default directory, markers and capabilities in its served-info fixture", () => {
    expect(served.id).toBe("pi");
    expect(served.label).toBe("Pi");
    expect(served.short_name).toBe("Pi");
    expect(served.install_url).toBe("https://github.com/earendil-works/pi");
    expect(served.default_config_dir).toBe(".pi/agent");
    expect(served.detection_markers).toEqual([".pi", "AGENTS.md", ".agents"]);
    expect(served.capabilities.preallocates_session_id).toBe(true);
    expect(served.capabilities.owned_accounts).toBe(true);
    expect(served.capabilities.hook_script).toBe(true);
    expect(served.capabilities.ends_command_groups_on_exit).toBe(true);
    expect(served.capabilities.login_flow).toBe(true);
    expect(served.capabilities.message_images).toBe(true);
    expect(served.capabilities.slash_commands).toBe(true);
    expect(served.capabilities.interactions).toEqual({ permission: false, question: false });
    expect(served.native_memory).toBe("absent");
    expect(served.settings).toEqual([]);
    expect(served.quota_windows.kinds).toEqual([]);
  });
});
