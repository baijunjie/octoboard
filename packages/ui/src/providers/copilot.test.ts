import { describe, expect, it } from "vitest";

import { copilot } from "./copilot";
import { served } from "./copilot.fixture";
import { loginQuestionKeys } from "../agents/providers";
import { providerColour, providerMark, providerPresentation } from "./registry";

describe("the GitHub Copilot CLI presentation module", () => {
  it("is the registered module of its id, and draws its mark in one path with the text colour as its fill", () => {
    expect(providerPresentation("copilot")).toBe(copilot);
    expect(providerMark("copilot")).toBe(copilot.mark);
    expect(copilot.mark.fill).toBe("currentColor");
    expect(copilot.mark.d).toMatch(/^[MmLlHhVvCcSsQqTtAaZz0-9\s.,-]+$/);
  });

  it("carries no colour, so its bars draw in the text colour", () => {
    expect(providerColour("copilot")).toBeUndefined();
    expect("colour" in copilot).toBe(false);
  });

  it("names itself the same in every language, and offers its brand name as a placeholder", () => {
    expect(copilot.params).toEqual({ copilot: "GitHub Copilot CLI" });
    const languages = Object.entries(copilot.messages);
    expect(languages.length).toBeGreaterThan(1);
    for (const [language, messages] of languages) {
      expect(messages["provider.copilot.label"], language).toBe("GitHub Copilot CLI");
      expect(messages["provider.copilot.shortName"], language).toBe("Copilot");
    }
  });

  it("words its permission setting in every language, with the same keys as English", () => {
    const keys = Object.keys(copilot.messages.en).sort();
    expect(keys).toContain("provider.copilot.setting.approval_mode.heading");
    expect(keys).toContain("provider.copilot.setting.approval_mode.choice.allow_all");
    for (const [language, messages] of Object.entries(copilot.messages)) {
      expect(Object.keys(messages).sort(), language).toEqual(keys);
      for (const [key, text] of Object.entries(messages)) expect(text.trim(), `${language} ${key}`).not.toBe("");
    }
  });

  it("words the permission setting in English: its heading, its one choice and the consequence the confirmation tells", () => {
    const en = copilot.messages.en;
    expect(en["provider.copilot.setting.approval_mode.heading"]).toBe("Permission approval");
    expect(en["provider.copilot.setting.approval_mode.choice.allow_all"]).toBe("Allow all");
    expect(en["provider.copilot.setting.approval_mode.confirm.allow_all.consequence"]).toContain("{agent}");
  });

  it("carries the copy of every setting, choice and confirmation its served-info fixture declares, in every language", () => {
    for (const setting of served.settings) {
      expect(setting.kind.type).toBe("choice");
      if (setting.kind.type !== "choice") continue;
      const wanted = [
        `provider.copilot.setting.${setting.id}.heading`,
        `provider.copilot.setting.${setting.id}.label`,
        ...setting.kind.choices.flatMap((choice) => [
          `provider.copilot.setting.${setting.id}.choice.${choice.value}`,
          `provider.copilot.setting.${setting.id}.choice.${choice.value}.summary`,
          ...(choice.confirm
            ? [
              `provider.copilot.setting.${setting.id}.confirm.${choice.value}.title`,
              `provider.copilot.setting.${setting.id}.confirm.${choice.value}.consequence`,
            ]
            : []),
        ]),
      ];
      for (const [language, messages] of Object.entries(copilot.messages)) {
        for (const key of wanted) expect(messages, `${language} ${key}`).toHaveProperty([key]);
      }
    }
  });

  it("words the confirmation's title in English and in every language", () => {
    const key = "provider.copilot.setting.approval_mode.confirm.allow_all.title";
    expect(copilot.messages.en[key]).toBe("Always approve tool calls?");
    expect(Object.keys(copilot.messages)).toHaveLength(17);
    for (const [language, messages] of Object.entries(copilot.messages)) {
      expect(messages[key]?.trim(), language).toBeTruthy();
    }
  });

  it("keeps the {agent} reference of the confirmation's consequence in every language", () => {
    for (const [language, messages] of Object.entries(copilot.messages)) {
      expect(messages["provider.copilot.setting.approval_mode.confirm.allow_all.consequence"], language).toContain("{agent}");
    }
  });

  it("states its names, install address, default directory, markers and capabilities in its served-info fixture", () => {
    expect(served.id).toBe("copilot");
    expect(served.label).toBe("GitHub Copilot CLI");
    expect(served.short_name).toBe("Copilot");
    expect(served.install_url).toBe("https://github.com/github/copilot-cli");
    expect(served.default_config_dir).toBe(".copilot");
    expect(served.detection_markers).toEqual(["AGENTS.md"]);
    expect(served.capabilities.preallocates_session_id).toBe(true);
    expect(served.capabilities.owned_accounts).toBe(true);
    expect(served.capabilities.login_flow).toBe(true);
    expect(served.settings.map((setting) => setting.id)).toEqual(["approval_mode"]);
    expect(served.settings[0].kind).toEqual({ type: "choice", choices: [{ value: "allow_all", confirm: true }] });
    expect(served.settings[0].shown_to_agents).toBe(false);
    expect(served.capabilities.mcp_inventory).toBe(true);
    expect(served.quota_windows.kinds).toEqual(["credit_period"]);
  });

  it("words its login's plain-text question and both answers in every language, naming the file it sends as {file} and nothing else", () => {
    const key = "provider.copilot.loginQuestion.keep_token_in_plain_text";
    expect(loginQuestionKeys("copilot", "keep_token_in_plain_text")).toEqual({ question: key, yes: `${key}.yes`, no: `${key}.no` });
    for (const [language, messages] of Object.entries(copilot.messages) as [string, Record<string, string>][]) {
      expect([...messages[key]!.matchAll(/\{([A-Za-z][A-Za-z0-9_]*)\}/g)].map((match) => match[1]), language).toEqual(["file"]);
      for (const answer of [`${key}.yes`, `${key}.no`]) {
        expect(messages[answer]?.trim(), `${language} ${answer}`).toBeTruthy();
        expect(messages[answer], `${language} ${answer}`).not.toMatch(/\{/);
      }
      expect(messages[`${key}.yes`], language).not.toBe(messages[`${key}.no`]);
    }
    // The dialog has no consequence paragraph: the question says itself that the token would be kept in plain text, and
    // the buttons name what they do.
    expect(copilot.messages.en[key]).toBe("No system keychain is available to store the login credentials. Store them in plain text in {file} instead? If you do not, the login cannot finish.");
    expect(copilot.messages.en[`${key}.yes`]).toBe("Store in plain text");
    expect(copilot.messages.en[`${key}.no`]).toBe("Do not store");
  });

  it("declares its login failure with a remedy of its own, worded in every language with no placeholder", () => {
    const code = "provider.copilot.token_not_kept";
    expect(copilot.codes).toEqual({ [code]: { origin: "daemon", recovery: `${code}.remedy` } });
    for (const [language, messages] of Object.entries(copilot.messages) as [string, Record<string, string>][]) {
      for (const key of [code, `${code}.remedy`]) {
        expect(messages[key]?.trim(), `${language} ${key}`).toBeTruthy();
        expect(messages[key], `${language} ${key}`).not.toMatch(/\{/);
      }
    }
    expect(copilot.messages.en[code]).toBe("You chose not to store the login credentials in plain text, and no system keychain is available to store them.");
    // The question can be put on a computer other than the one sessions run on: the remedy says where.
    expect(copilot.messages.en[`${code}.remedy`]).toContain("in the environment where sessions run");
    // "token" is the context window's unit in the app; the login's secret is its login credentials.
    for (const [language, messages] of Object.entries(copilot.messages) as [string, Record<string, string>][]) {
      for (const key of ["provider.copilot.loginQuestion.keep_token_in_plain_text", code, `${code}.remedy`]) {
        expect(messages[key], `${language} ${key}`).not.toMatch(/token|トークン|토큰|โทเค็น|टोकन|токен|الرمز المميز|jeton|令牌|權杖/i);
      }
    }
  });
});
