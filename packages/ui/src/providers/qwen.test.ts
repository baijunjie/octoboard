import { describe, expect, it } from "vitest";

import { qwen } from "./qwen";
import { served } from "./qwen.fixture";
import { providerColour, providerMark, providerPresentation } from "./registry";

describe("the Qwen Code presentation module", () => {
  it("is the registered module of its id, and draws its mark in one path with the text colour as its fill", () => {
    expect(providerPresentation("qwen")).toBe(qwen);
    expect(providerMark("qwen")).toBe(qwen.mark);
    expect(qwen.mark.fill).toBe("currentColor");
    expect(qwen.mark.d).toMatch(/^[MmLlHhVvCcSsQqTtAaZz0-9\s.,-]+$/);
  });

  it("carries its colour in each appearance", () => {
    expect(providerColour("qwen")).toEqual({ light: "#6a4cf0", dark: "#a594ff" });
    expect(qwen.colour).toEqual({ light: "#6a4cf0", dark: "#a594ff" });
  });

  it("names itself the same in every language, and offers its brand name as a placeholder", () => {
    expect(qwen.params).toEqual({ qwen: "Qwen Code" });
    const languages = Object.entries(qwen.messages);
    expect(languages.length).toBeGreaterThan(1);
    for (const [language, messages] of languages) {
      expect(messages, language).toMatchObject({ "provider.qwen.label": "Qwen Code", "provider.qwen.shortName": "Qwen" });
    }
  });

  it("words the same messages in every language, none of them empty", () => {
    const keys = Object.keys(qwen.messages.en).sort();
    for (const [language, messages] of Object.entries(qwen.messages)) {
      expect(Object.keys(messages).sort(), language).toEqual(keys);
      for (const [key, text] of Object.entries(messages)) expect(text.trim(), `${language} ${key}`).not.toBe("");
    }
  });

  it("keeps the placeholders of every message the same in every language", () => {
    const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();
    for (const [key, english] of Object.entries(qwen.messages.en)) {
      for (const [language, messages] of Object.entries(qwen.messages)) {
        expect(placeholders(messages[key as keyof typeof messages]), `${language} ${key}`).toEqual(placeholders(english));
      }
    }
  });

  it("words its sign-in hint in every language, naming the command, the variable, the account's directory and the in-app command", () => {
    expect(qwen.references.signInHint).toEqual({ command: "qwen", variable: "QWEN_HOME", authCommand: "/auth" });
    for (const [language, messages] of Object.entries(qwen.messages)) {
      const hint = messages["provider.qwen.signInHint"];
      for (const placeholder of ["{command}", "{variable}", "{dir}", "{authCommand}"]) expect(hint, `${language} ${placeholder}`).toContain(placeholder);
    }
  });

  it("words each choice of its permission setting, and the confirmation only of the choice its descriptor asks one for", () => {
    const setting = served.settings.find((candidate) => candidate.id === "approval_mode");
    if (setting?.kind.type !== "choice") throw new Error("the fixture declares no approval_mode choice");
    expect(setting.kind.choices.map((choice) => choice.value)).toEqual(["plan", "default", "auto_edit", "auto", "yolo"]);
    for (const [language, messages] of Object.entries(qwen.messages)) {
      const keys = Object.keys(messages);
      for (const name of ["heading", "helpLabel", "label", "saveFailedTitle", "helpTiming", "helpCustom", "inheritSummary"]) {
        expect(keys, `${language} ${name}`).toContain(`provider.qwen.setting.approval_mode.${name}`);
      }
      for (const choice of setting.kind.choices) {
        const prefix = `provider.qwen.setting.approval_mode.choice.${choice.value}`;
        expect(keys, `${language} ${choice.value}`).toEqual(expect.arrayContaining([prefix, `${prefix}.summary`]));
        const confirm = `provider.qwen.setting.approval_mode.confirm.${choice.value}`;
        for (const part of ["title", "consequence"]) {
          expect(keys.includes(`${confirm}.${part}`), `${language} ${choice.value} confirm ${part}`).toBe(choice.confirm);
        }
      }
    }
  });

  it("declares the error its descriptor does with its parameter and its remedy, and words both in every language", () => {
    const code = "provider.qwen.system_settings_unusable";
    const remedy = `${code}.remedy` as const;
    // The message says which file, as the daemon sends it, and which agent it is the system settings of. It states the
    // cause only: it is the reason a launch started without the app's additions, never a refused launch.
    expect(qwen.codes[code]).toEqual({ origin: "daemon", recovery: remedy, codeParams: ["path"], uiParams: { agent: "Qwen Code" } });
    expect(qwen.messages.en[code]).toBe("The {agent} system settings file {path} cannot be read as settings.");
    for (const [language, messages] of Object.entries(qwen.messages)) {
      expect(messages[code], `${language} ${code}`).toContain("{path}");
      expect(messages[code], `${language} ${code}`).toContain("{agent}");
      expect(messages[remedy].trim(), `${language} ${remedy}`).not.toBe("");
    }
  });

  // A notice is told at a launch that goes on, so it has no remedy of its own: its sentence says what to do. Both say
  // that a protection is not in force and name the step that turns it back on, so both stay until dismissed, under a
  // title.
  it("declares its two launch notices as daemon codes to acknowledge with no remedy, each worded in every language with the parameters the daemon sends", () => {
    const notices = [
      // Hooks switched off by a variable of the session's environment, named as the daemon sends it.
      { code: "provider.qwen.hooks_off", params: ["switch"] },
      // Hooks switched off by a setting: the setting, and the settings file that holds it.
      { code: "provider.qwen.hooks_off_by_setting", params: ["switch", "path"] },
    ] as const;
    expect(Object.keys(qwen.codes)).toEqual(["provider.qwen.system_settings_unusable", ...notices.map(({ code }) => code)]);
    for (const { code, params } of notices) {
      expect(qwen.codes[code]).toEqual({ origin: "daemon", acknowledgedNotice: true, alertTitle: true, codeParams: [...params], uiParams: { agent: "Qwen Code" } });
      for (const [language, messages] of Object.entries(qwen.messages)) {
        expect(Object.keys(messages), `${language} ${code}`).not.toContain(`${code}.remedy`);
        // A short title, a plain name with nothing filled in.
        const title = messages[`${code}.alertTitle`];
        expect(title.trim(), `${language} ${code} title`).not.toBe("");
        expect(title, `${language} ${code} title`).not.toMatch(/[{}]/);
        expect(title.length, `${language} ${code} title`).toBeLessThan(30);
        // Each name is referenced once.
        for (const placeholder of ["agent", ...params]) {
          expect(messages[code].split(`{${placeholder}}`), `${language} ${code} ${placeholder}`).toHaveLength(2);
        }
      }
    }
  });

  // A prompt the agent waits on is named by the core catalog's own word for it
  // (`daemon.provider_unsupported.interactions`), never by the word for the user's prompt or for a session request.
  it("names the prompts its notices say are answered only in the terminal by the core catalog's word", () => {
    const words = {
      "zh-CN": "其询问也只能在终端中回应",
      "zh-TW": "其詢問也只能在終端機中回應",
      ja: "問いかけにはターミナルでしか回答できません",
      ko: "요청",
      de: "Rückfragen",
      hi: "सवाल",
      th: "คำถาม",
      vi: "lời nhắc",
      ar: "طلباتها",
    } as const;
    for (const [language, word] of Object.entries(words)) {
      const messages = qwen.messages[language as keyof typeof qwen.messages];
      for (const code of ["provider.qwen.hooks_off", "provider.qwen.hooks_off_by_setting"] as const) {
        expect(messages[code], `${language} ${code}`).toContain(word);
      }
    }
  });

  it("states its names, install address, default directory, markers and capabilities in its served-info fixture", () => {
    expect(served.id).toBe("qwen");
    expect(served.label).toBe("Qwen Code");
    expect(served.short_name).toBe("Qwen");
    expect(served.install_url).toBe("https://github.com/QwenLM/qwen-code");
    expect(served.default_config_dir).toBe(".qwen");
    expect(served.detection_markers).toEqual([".qwen", "QWEN.md", "AGENTS.md", ".agents"]);
    expect(served.capabilities.preallocates_session_id).toBe(true);
    expect(served.capabilities.owned_accounts).toBe(true);
    expect(served.capabilities.ends_command_groups_on_exit).toBe(true);
    expect(served.capabilities.interactions).toEqual({ permission: true, question: true });
    expect(served.capabilities.message_images).toBe(true);
    expect(served.capabilities.slash_commands).toBe(true);
    expect(served.native_memory).toBe("switchable");
    expect(served.capabilities.mcp_inventory).toBe(true);
    expect(served.capabilities.login_flow).toBe(true);
    expect(served.settings.map((setting) => setting.id)).toEqual(["approval_mode", "claude_instructions"]);
    expect(served.quota_windows.kinds).toEqual([]);
  });
});
