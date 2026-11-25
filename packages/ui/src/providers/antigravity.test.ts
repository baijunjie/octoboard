import { describe, expect, it } from "vitest";

import { LANGUAGES } from "../i18n/languages";
import { antigravity } from "./antigravity";
import { served } from "./antigravity.fixture";
import { claude } from "./claude";
import { opencode } from "./opencode";
import { providerColour, providerMark, providerPresentation } from "./registry";

type Catalogs = Record<string, Record<string, string>>;

const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();

describe("the Antigravity CLI presentation module", () => {
  it("is the registered module of its id, and draws its mark in one path with the text colour as its fill", () => {
    expect(providerPresentation("antigravity")).toBe(antigravity);
    expect(providerMark("antigravity")).toBe(antigravity.mark);
    expect(antigravity.mark.fill).toBe("currentColor");
    expect(antigravity.mark.d).toMatch(/^[MmLlHhVvCcSsQqTtAaZz0-9\s.,-]+$/);
  });

  // The user has not confirmed the colours yet: they are pinned as the module's present values.
  it("carries its colour in each appearance", () => {
    expect(providerColour("antigravity")).toEqual({ light: "#0d8a4c", dark: "#3ccf86" });
    expect(antigravity.colour).toEqual({ light: "#0d8a4c", dark: "#3ccf86" });
  });

  it("names itself the same in every language Octoboard offers, and offers its brand name as a placeholder", () => {
    expect(antigravity.params).toEqual({ antigravity: "Antigravity CLI" });
    expect(Object.keys(antigravity.messages).sort()).toEqual([...LANGUAGES].sort());
    for (const [language, messages] of Object.entries(antigravity.messages)) {
      expect(messages["provider.antigravity.label"], language).toBe("Antigravity CLI");
      expect(messages["provider.antigravity.shortName"], language).toBe("Antigravity");
    }
  });

  it("words the same messages in every language, none of them empty", () => {
    const keys = Object.keys(antigravity.messages.en).sort();
    expect(keys.length).toBeGreaterThan(2);
    for (const [language, messages] of Object.entries(antigravity.messages)) {
      expect(Object.keys(messages).sort(), language).toEqual(keys);
      for (const [key, text] of Object.entries(messages)) expect(text.trim(), `${language} ${key}`).not.toBe("");
    }
  });

  it("keeps the placeholders of every message the same in every language", () => {
    for (const [key, english] of Object.entries(antigravity.messages.en)) {
      for (const [language, messages] of Object.entries(antigravity.messages)) {
        expect(placeholders(messages[key as keyof typeof messages]), `${language} ${key}`).toEqual(placeholders(english));
      }
    }
  });

  it("words its sign-in hint in every language, naming the command to run", () => {
    expect(antigravity.references.signInHint).toEqual({ command: "agy" });
    for (const [language, messages] of Object.entries(antigravity.messages)) {
      const hint = messages["provider.antigravity.signInHint"];
      expect(placeholders(hint), language).toEqual(["command"]);
    }
  });

  it("declares its four refusals as daemon codes, each worded with a remedy in every language, naming the parameter the daemon sends", () => {
    const flag = "provider.antigravity.launch_directory_flag_not_taken";
    const flagRemedy = "provider.antigravity.launch_directory_flag_not_taken.remedy";
    const unusable = "provider.antigravity.user_config_unusable";
    const unusableRemedy = "provider.antigravity.user_config_unusable.remedy";
    const unmigrated = "provider.antigravity.earlier_layout_not_migrated";
    const unmigratedRemedy = "provider.antigravity.earlier_layout_not_migrated.remedy";
    const bothPlaces = "provider.antigravity.earlier_layout_file_in_both_places";
    const bothPlacesRemedy = "provider.antigravity.earlier_layout_file_in_both_places.remedy";
    const settingsNotRead = "provider.antigravity.settings_not_read";
    const leftoversNotRead = "provider.antigravity.earlier_layout_leftovers_not_read";
    expect(Object.keys(antigravity.codes)).toEqual([flag, unusable, unmigrated, bothPlaces, settingsNotRead, leftoversNotRead]);
    // The daemon names the file that would be replaced and the earlier one, each drawn as a reference.
    expect(antigravity.codes[bothPlaces]).toEqual({
      origin: "daemon",
      recovery: bothPlacesRemedy,
      codeParams: ["path", "earlier_path"],
      uiParams: { agent: "Antigravity CLI" },
    });
    // The daemon sends no parameter with it; the remedy's command is the module's own reference.
    expect(antigravity.codes[unmigrated]).toEqual({
      origin: "daemon",
      recovery: unmigratedRemedy,
      uiParams: { agent: "Antigravity CLI" },
    });
    expect(antigravity.recoveryReferences).toEqual({ command: "agy" });
    expect(antigravity.codes[flag]).toEqual({
      origin: "daemon",
      recovery: flagRemedy,
      codeParams: ["version"],
      uiParams: { agent: "Antigravity CLI" },
    });
    expect(antigravity.codes[unusable]).toEqual({
      origin: "daemon",
      recovery: unusableRemedy,
      codeParams: ["path"],
      uiParams: { agent: "Antigravity CLI" },
    });
    for (const [language, messages] of Object.entries(antigravity.messages)) {
      for (const name of [flag, flagRemedy, unusable, unusableRemedy, unmigrated, unmigratedRemedy, bothPlaces, bothPlacesRemedy] as const) {
        expect(messages[name].trim(), `${language} ${name}`).not.toBe("");
      }
      // The message names the version that takes the option; the remedy names the agent alone.
      expect(placeholders(messages[flagRemedy]), `${language} flag remedy`).toEqual(["agent"]);
      expect(placeholders(messages[flag]), `${language} flag message`).toEqual(["agent", "appName", "version"]);
      expect(placeholders(messages[unusable]), `${language} unusable message`).toEqual(["agent", "path"]);
      expect(placeholders(messages[unusableRemedy]), `${language} unusable remedy`).toEqual([]);
      // The message names the agent alone; the remedy names the command to run and nothing else.
      expect(placeholders(messages[unmigrated]), `${language} unmigrated message`).toEqual(["agent"]);
      expect(placeholders(messages[unmigratedRemedy]), `${language} unmigrated remedy`).toEqual(["command"]);
      // The message names both files, once each. The remedy names them by version, not by path, and names only the
      // command, twice: for what running it with both files there does, and to run.
      expect(placeholders(messages[bothPlaces]), `${language} both places message`).toEqual(["agent", "earlier_path", "path"]);
      expect(placeholders(messages[bothPlacesRemedy]), `${language} both places remedy`).toEqual(["command", "command"]);
    }
  });

  // The module's choice: its remedies name the agent and the module's own references, and leave a code's parameters
  // to the message. A placeholder nothing fills is drawn as written, braces and all.
  it("uses in a remedy no placeholder but the agent and the module's own references, in every language", () => {
    const filled = ["agent", ...Object.keys(antigravity.recoveryReferences)];
    let remedies = 0;
    for (const [language, messages] of Object.entries(antigravity.messages)) {
      for (const [key, text] of Object.entries(messages)) {
        if (!key.endsWith(".remedy")) continue;
        remedies += 1;
        for (const placeholder of placeholders(text)) expect(filled, `${language} ${key} {${placeholder}}`).toContain(placeholder);
      }
    }
    const refusals = Object.values(antigravity.codes).filter((record) => "recovery" in record);
    expect(refusals).toHaveLength(4);
    expect(remedies).toBe(refusals.length * LANGUAGES.length);
    for (const record of refusals) expect(Object.keys(antigravity.messages.en)).toContain(record.recovery);
  });

  // A notice is told at a launch that goes on, so it has no remedy of its own: its sentence says what to do. Both say
  // that something of the user's own is not in force and name a path, so both are marked to stay until dismissed.
  it("declares its two launch notices as daemon codes to acknowledge with no remedy, each worded in every language with the parameters the daemon sends", () => {
    const settingsNotRead = "provider.antigravity.settings_not_read";
    const leftoversNotRead = "provider.antigravity.earlier_layout_leftovers_not_read";
    expect(antigravity.codes[settingsNotRead]).toEqual({ origin: "daemon", acknowledgedNotice: true, alertTitle: true, codeParams: ["path"], uiParams: { agent: "Antigravity CLI" } });
    // The two folder names are literals of the module's own, drawn as references like the two paths the daemon sends.
    const { variantOf, ...leftovers } = antigravity.codes[leftoversNotRead];
    expect(leftovers).toEqual({
      origin: "daemon",
      acknowledgedNotice: true,
      alertTitle: true,
      codeParams: ["path", "current_path", "rules", "plugins"],
      uiParams: { agent: "Antigravity CLI", rules: "rules", plugins: "plugins" },
      variants: { one: { codeParams: ["path", "current_path", "left"] }, both: { codeParams: ["path", "current_path", "rules", "plugins"] } },
    });
    // The daemon says which of the two folders is left, and the sentence names only what is there: the one folder by
    // the daemon's own word for it, both, or, for a word this window does not know, the sentence that holds for either.
    expect(variantOf({ left: "rules" })).toBe("one");
    expect(variantOf({ left: "plugins" })).toBe("one");
    expect(variantOf({ left: "both" })).toBe("both");
    expect(variantOf({ left: "skills" })).toBeUndefined();
    expect(variantOf({})).toBeUndefined();
    for (const [language, messages] of Object.entries(antigravity.messages)) {
      // Each has a title of its own, a plain name with nothing filled in, so two of one launch do not read alike.
      const settingsTitle = messages[`${settingsNotRead}.alertTitle`];
      const leftoversTitle = messages[`${leftoversNotRead}.alertTitle`];
      expect(settingsTitle.trim(), `${language} settings title`).not.toBe("");
      expect(leftoversTitle.trim(), `${language} leftovers title`).not.toBe("");
      expect(settingsTitle, `${language} titles`).not.toBe(leftoversTitle);
      expect(placeholders(settingsTitle), `${language} settings title`).toEqual([]);
      expect(placeholders(leftoversTitle), `${language} leftovers title`).toEqual([]);
      // The wording for one folder left has a title of its own, in the singular where the language has one.
      const oneTitle = messages[`${leftoversNotRead}.one.alertTitle`];
      expect(oneTitle.trim(), `${language} one-folder title`).not.toBe("");
      expect(placeholders(oneTitle), `${language} one-folder title`).toEqual([]);
      expect(placeholders(messages[settingsNotRead]), `${language} settings`).toEqual(["agent", "path"]);
      expect(placeholders(messages[leftoversNotRead]), `${language} leftovers`).toEqual(["agent", "current_path", "path", "plugins", "rules"]);
      expect(placeholders(messages[`${leftoversNotRead}.one`]), `${language} one left`).toEqual(["agent", "current_path", "left", "path"]);
      expect(placeholders(messages[`${leftoversNotRead}.both`]), `${language} both left`).toEqual(["agent", "current_path", "path", "plugins", "rules"]);
      expect(Object.keys(messages), language).not.toContain(`${settingsNotRead}.remedy`);
      expect(Object.keys(messages), language).not.toContain(`${leftoversNotRead}.remedy`);
    }
    // A path is named once in each, so the folder to move things into is said without it.
    expect(antigravity.messages.en[`${leftoversNotRead}.one.alertTitle`]).toBe("Earlier folder not read");
    expect(antigravity.messages.en[`${leftoversNotRead}.one`]).toBe(
      "{agent} does not read the {left} folder that an earlier version left in {path}. It reads only the one in {current_path}, so this session runs without anything that is only in the earlier folder. To keep something from the earlier folder, move it into the folder that is read.",
    );
    // The settings notice says why the file needs repair, and to launch the session again as the refusals' remedies do.
    expect(antigravity.messages.en[settingsNotRead]).toBe(
      "{agent} cannot read its settings file {path} as settings, so this session runs on the default settings, without the file's permission rules and trusted folders. Repair the file, then launch the session again.",
    );
  });

  // What a session changes in its launch's own files is lost with it: the provider's note about its sessions says so,
  // with the way to keep such a change, naming the command and the global rule file as references of the note and the
  // app by its global name. The help of its one setting says nothing of it, and names no command.
  it("says in its session note, in every language, that a session's own configuration changes are lost and how to keep one", () => {
    expect(antigravity.references.sessionNote).toEqual({ command: "agy", ruleFile: "GEMINI.md" });
    expect("settingReferences" in antigravity).toBe(false);
    for (const [language, messages] of Object.entries(antigravity.messages)) {
      const note = messages["provider.antigravity.sessionNote"];
      expect(placeholders(note), language).toEqual(["agent", "appName", "command", "ruleFile"]);
      expect(note, language).toContain("MCP");
      expect(placeholders(messages["provider.antigravity.setting.approval_mode.helpCustom"]), language).toEqual(["agent"]);
    }
    expect(antigravity.messages.en["provider.antigravity.sessionNote"]).toBe(
      "Changes made inside a session of {agent} to its hooks, MCP servers, plugin choices or {ruleFile} are lost when the session ends. To keep such a change, run {command} directly, outside {appName}, and make it there.",
    );
  });

  it("words each choice of its permission setting, and the confirmation only of the choice its descriptor asks one for", () => {
    const setting = served.settings.find((candidate) => candidate.id === "approval_mode");
    if (setting?.kind.type !== "choice") throw new Error("the fixture declares no approval_mode choice");
    expect(setting.kind.choices).toEqual([
      { value: "accept_edits", confirm: false },
      { value: "auto", confirm: false },
      { value: "skip_permissions", confirm: true },
    ]);
    for (const [language, messages] of Object.entries(antigravity.messages)) {
      const keys = Object.keys(messages);
      for (const name of ["heading", "helpLabel", "label", "saveFailedTitle", "helpTiming", "helpCustom", "inheritSummary"]) {
        expect(keys, `${language} ${name}`).toContain(`provider.antigravity.setting.approval_mode.${name}`);
      }
      for (const choice of setting.kind.choices) {
        const prefix = `provider.antigravity.setting.approval_mode.choice.${choice.value}`;
        expect(keys, `${language} ${choice.value}`).toEqual(expect.arrayContaining([prefix, `${prefix}.summary`]));
        const confirm = `provider.antigravity.setting.approval_mode.confirm.${choice.value}`;
        for (const part of ["title", "consequence"]) {
          expect(keys.includes(`${confirm}.${part}`), `${language} ${choice.value} confirm ${part}`).toBe(choice.confirm);
        }
      }
    }
  });

  // The wording of skipping permissions and of its confirmation is not Antigravity's own: it is Claude's for bypassing
  // them and OpenCode's for approving every call, in every language, so a change there is made once.
  it("words skipping permissions as Claude words bypassing them, and its confirmation as OpenCode words approving every call", () => {
    const own = antigravity.messages as unknown as Catalogs;
    const claudeCopy = claude.messages as unknown as Catalogs;
    const opencodeCopy = opencode.messages as unknown as Catalogs;
    const prefix = "provider.antigravity.setting.approval_mode";
    expect(own.en![`${prefix}.choice.skip_permissions`]).toBe("Skip permissions");
    for (const language of LANGUAGES) {
      if (language !== "en") {
        expect(own[language]![`${prefix}.choice.skip_permissions`], `${language} choice`).toBe(
          claudeCopy[language]!["provider.claude.setting.approval_mode.choice.bypass"],
        );
      }
      for (const part of ["title", "consequence"]) {
        expect(own[language]![`${prefix}.confirm.skip_permissions.${part}`], `${language} confirm ${part}`).toBe(
          opencodeCopy[language]![`provider.opencode.setting.approval_mode.confirm.auto.${part}`],
        );
      }
    }
  });

  it("states its names, install address, default directory, markers and capabilities in its served-info fixture", () => {
    expect(served.id).toBe("antigravity");
    expect(served.label).toBe("Antigravity CLI");
    expect(served.short_name).toBe("Antigravity");
    expect(served.install_url).toBe("https://antigravity.google/product/antigravity-cli");
    expect(served.default_config_dir).toBe(".gemini");
    expect(served.detection_markers).toEqual(["GEMINI.md", "AGENTS.md", ".agents"]);
    expect(served.capabilities.preallocates_session_id).toBe(false);
    expect(served.capabilities.owned_accounts).toBe(false);
    expect(served.capabilities.message_images).toBe(true);
    expect(served.capabilities.slash_commands).toBe(true);
    expect(served.native_memory).toBe("absent");
    expect(served.settings.map((setting) => setting.id)).toEqual(["approval_mode"]);
    expect(served.capabilities.mcp_inventory).toBe(true);
    expect(served.quota_windows.kinds).toEqual(["weekly_model"]);
  });
});
