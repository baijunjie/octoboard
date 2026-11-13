import { afterEach, expect, it } from "vitest";

import { CATALOGS, format, messageText, splitPlaceholders, type Message, type MessageKey } from "./catalog";
import { appConfig } from "../appConfig";
import { browserHost } from "../platform/host";
import { MAC_HOST, WINDOWS_HOST } from "../test/hosts";
import { LANGUAGES, type Language } from "./languages";
import { en } from "./messages/en";
import { announceHost, PLATFORM_TERMS, platformParam, platformParamBase, type PlatformHost, VOCABULARY_PARAM_NAMES } from "./platformTerms";

it("fills placeholders and selects the plural form", () => {
  expect(format("en", "rail.waiting", { count: 1 })).toBe("1 session is waiting for you. Go to the next one.");
  expect(format("en", "menu.about")).toBe(`About ${appConfig.name}`);
});

it("isolates global brand names in plain RTL messages without putting controls in the catalog", () => {
  expect(messageText("ar", "menu.quit")).not.toMatch(/[\u2068\u2069]/);
  expect(format("ar", "menu.quit")).toContain(`\u2068${appConfig.name}\u2069`);
});

it("uses English plural rules when a catalog or message is unavailable", () => {
  const french = CATALOGS.fr;
  const registered = Object.hasOwn(CATALOGS, "fr");
  try {
    // French treats zero as singular; the English fallback must use its own plural rules.
    CATALOGS.fr = undefined;
    expect(format("fr", "rail.waiting", { count: 0 })).toBe("0 sessions are waiting for you. Go to the next one.");
    CATALOGS.fr = { ...french, "rail.waiting": undefined };
    expect(format("fr", "rail.waiting", { count: 0 })).toBe("0 sessions are waiting for you. Go to the next one.");
  } finally {
    if (registered) CATALOGS.fr = french;
    else delete CATALOGS.fr;
  }
});

it.each([0, 1, 2, 73866303])("localizes context size units for a total of %i", (count) => {
  const tokens = count.toLocaleString("en");
  const params = { count, tokens };
  expect(format("en", "session.context.heading", params)).toBe(`Context ${tokens} ${count === 1 ? "token" : "tokens"}`);
  expect(format("zh-Hans", "session.context.heading", params)).toBe(`上下文 ${tokens} 个 token`);
});

it("registers a complete catalog for every offered language", () => {
  expect(Object.keys(CATALOGS).sort()).toEqual([...LANGUAGES].sort());
  const keys = Object.keys(en).sort();
  for (const [language, catalog] of Object.entries(CATALOGS)) {
    expect(Object.keys(catalog).sort(), language).toEqual(keys);
    for (const [key, message] of Object.entries(catalog)) {
      for (const form of formsOf(message)) {
        // The separator is intentionally empty in languages with no space after a sentence.
        if (key === "common.sentenceSeparator") continue;
        expect(form.trim().length, `${language} ${key}`).toBeGreaterThan(0);
      }
    }
  }
});

it("omits ellipses beside the MCP refresh icon and keeps them in loading placeholders in every language", () => {
  for (const language of LANGUAGES) {
    expect(messageText(language, "settings.accounts.mcp.refreshing"), language).not.toMatch(/(?:…|\.{3})\s*$/);
    expect(messageText(language, "common.loading"), language).toMatch(/…$/);
  }
});

const translated = Object.entries(CATALOGS).filter(([language]) => language !== "en") as [
  Language,
  Record<string, Message>,
][];

function formsOf(message: Message): string[] {
  return typeof message === "string" ? [message] : Object.values(message);
}

it("names the Appearance section and describes it in a configuration export in every language, and no longer names an App behavior section", () => {
  for (const [language, catalog] of Object.entries(CATALOGS)) {
    const messages = catalog as Record<string, Message>;
    expect(formsOf(messages["settings.section.appearance"]!).every((form) => form.trim().length > 0), language).toBe(true);
    expect(formsOf(messages["configuration.export.appearance.description"]!).every((form) => form.trim().length > 0), language).toBe(true);
    expect(Object.keys(messages), language).not.toContain("settings.section.appBehavior");
    expect(Object.keys(messages), language).not.toContain("configuration.export.appBehavior.description");
  }
});

/** The placeholders of a message's text; a platform word asked for in a form or capitalised counts as the plain one,
 * which is how the English message names it. */
function placeholdersOf(text: string): Set<string> {
  return new Set(splitPlaceholders(text).filter((_, index) => index % 2 === 1).map((name) => platformParamBase(name) ?? name));
}

it("gives each plural message exactly the categories its language needs", () => {
  for (const [language, catalog] of translated) {
    const categories = new Intl.PluralRules(language).resolvedOptions().pluralCategories.sort();
    for (const [key, message] of Object.entries(catalog)) {
      if (typeof message === "string") continue;
      expect(Object.keys(message).sort(), `${language} ${key}`).toEqual(categories);
    }
  }
});

it("keeps every translated message's placeholders those of the English one", () => {
  for (const [language, catalog] of translated) {
    for (const [key, message] of Object.entries(catalog)) {
      const english = en[key as MessageKey] as Message;
      const expected = new Set(formsOf(english).flatMap((form) => [...placeholdersOf(form)]));
      for (const form of formsOf(message)) {
        const actual = placeholdersOf(form);
        // A plural form may leave out `{count}` (a word for "one" needs no number), but never add one.
        const required = typeof message === "string" ? expected : new Set([...expected].filter((name) => name !== "count"));
        const missing = [...required].filter((name) => !actual.has(name));
        // A vocabulary word needs no caller, and a language asks for one where English words the thing alike on every system.
        const extra = [...actual].filter((name) => !expected.has(name) && !(VOCABULARY_PARAM_NAMES as readonly string[]).includes(name));
        expect({ missing, extra }, `${language} ${key}`).toEqual({
          missing: [],
          extra: [],
        });
      }
    }
  }
});

// The gallery names a change row by `git.row`'s text and lets whatever follows it be (`Ui.changeRow`),
// so a language that reorders a placeholder in another form would leave its rows unfindable there.
// The forms are the other `git.row` messages that carry the status.
it("begins every form of a change row's name with the plain one", () => {
  const forms = (Object.keys(en) as MessageKey[]).filter((key) => /^git\.row\./.test(key) && String(en[key]).includes("{status}"));
  expect(forms.length).toBeGreaterThan(0);
  for (const language of Object.keys(CATALOGS) as Language[]) {
    const plain = messageText(language, "git.row");
    for (const key of forms) {
      expect(messageText(language, key).slice(0, plain.length), `${language} ${key}`).toBe(plain);
    }
  }
});

// The gallery names a session row by what precedes its agent and status (`Ui.session`), which is the
// title's own prefix only while `{title}` comes first.
it("puts a session row's title before its agent and status", () => {
  for (const language of Object.keys(CATALOGS) as Language[]) {
    for (const key of ["sidebar.session.ariaLabel", "sidebar.session.ariaLabelPinned"] as const) {
      const text = messageText(language, key);
      const title = text.indexOf("{title}");
      expect(title, `${language} ${key}`).toBeGreaterThanOrEqual(0);
      expect(title, `${language} ${key}`).toBeLessThan(text.indexOf("{agent}"));
      expect(title, `${language} ${key}`).toBeLessThan(text.indexOf("{status}"));
    }
  }
});

// The hosts a message can be shown on: the platform words are filled from the host the shell announced.
const LINUX_HOST: PlatformHost = browserHost("Linux x86_64");
const HOSTS: [string, PlatformHost][] = [["macOS", MAC_HOST], ["Windows", WINDOWS_HOST], ["Linux", LINUX_HOST]];

afterEach(() => announceHost(MAC_HOST));

it("fills every platform word a message uses, in every language, for every host", () => {
  for (const [hostName, host] of HOSTS) {
    announceHost(host);
    for (const [language, catalog] of Object.entries(CATALOGS) as [Language, Record<string, Message>][]) {
      for (const [key, message] of Object.entries(catalog)) {
        for (const form of formsOf(message)) {
          for (const name of splitPlaceholders(form).filter((_, index) => index % 2 === 1)) {
            if (platformParamBase(name) === undefined) continue;
            const text = platformParam(name, language, host);
            expect(text?.trim().length, `${hostName} ${language} ${key} {${name}}`).toBeGreaterThan(0);
            expect(text, `${hostName} ${language} ${key} {${name}}`).not.toMatch(/[{}]/);
          }
        }
        // Nothing is left unfilled: the placeholders still in the text are the caller's, which this test does not pass.
        const left = splitPlaceholders(format(language, key as MessageKey, { count: 2 })).filter((_, index) => index % 2 === 1);
        for (const name of left) expect(platformParamBase(name) ?? (name === "osName" ? name : undefined), `${hostName} ${language} ${key} {${name}}`).toBeUndefined();
      }
    }
  }
});

it("has the terms every host asks for in every language", () => {
  for (const language of LANGUAGES) {
    for (const [hostName, host] of HOSTS) {
      for (const key of [host.terms.trash, host.terms.settings, host.terms.menuBarIcon, "windowsSettings", "developerMode"] as const) {
        expect(PLATFORM_TERMS[language][key], `${hostName} ${language} ${key}`).toBeTruthy();
      }
    }
  }
});

it("words the operating system and the system's own terms for the host", () => {
  announceHost(MAC_HOST);
  expect(format("en", "dialog.removeProject.trash")).toBe("Also move the project folder to the Trash");
  expect(format("en", "settings.notifications.managedBySystem")).toContain("macOS manages these notifications");
  announceHost(WINDOWS_HOST);
  expect(format("en", "dialog.removeProject.trash")).toBe("Also move the project folder to the Recycle Bin");
  expect(format("en", "settings.notifications.openSettings")).toBe("Open Windows Settings");
  expect(format("en", "settings.background.description.on")).toContain("The notification area icon brings the window back.");
  expect(format("en", "settings.notifications.managedBySystem")).toContain("Windows manages these notifications");
  // A form follows the host's word: Portuguese's article, Turkish's vowel harmony, Russian's case.
  expect(format("pt", "dialog.removeProject.trashHint", { git: "Git" })).toContain("da Lixeira");
  expect(format("tr", "dialog.removeProject.trash")).toBe("Proje klasörünü de Geri Dönüşüm Kutusu’na taşı");
  expect(format("tr", "settings.notifications.openSettings")).toBe("Windows Ayarları’nı aç");
  expect(format("ru", "dialog.removeProject.trash")).toBe("Также переместить папку проекта в Корзину");
  // A message that starts with the word asks for it capitalised.
  expect(format("tr", "settings.background.description.on")).toContain("Bildirim alanı simgesi pencereyi geri getirir.");
  announceHost(MAC_HOST);
  expect(format("tr", "settings.background.description.on")).toContain("Menü çubuğu simgesi pencereyi geri getirir.");
});

it("gives both Russian settings apps their prepositional form", () => {
  expect(platformParam("systemSettingsIn", "ru", WINDOWS_HOST)).toBe("Параметрах");
  expect(platformParam("systemSettingsIn", "ru", MAC_HOST)).toBe("Системных настройках");
  expect(platformParam("systemSettings", "ru", MAC_HOST)).toBe("Системные настройки");
  expect(platformParam("systemSettingsIn", "en", WINDOWS_HOST)).toBe("Settings");
});

// Each language's copy where macOS and Windows word a thing differently, which a vocabulary word or the settings app's
// full name carries: the macOS text is the one the catalogs had before, and Windows reads its own.
it.each([
  ["en", "settings.notifications.openSettings", "Open System Settings", "Open Windows Settings"],
  ["ja", "settings.notifications.openSettings", "システム設定を開く", "Windows の設定を開く"],
  ["zh-Hans", "settings.notifications.openSettings", "打开系统设置", "打开 Windows 设置"],
  ["ru", "settings.notifications.openSettings", "Открыть Системные настройки", "Открыть Параметры Windows"],
  ["de", "settings.notifications.systemPane", "Mitteilungen", "Benachrichtigungen"],
  ["ar", "settings.notifications.systemPane", "الإشعارات", "الإعلامات"],
  ["hi", "settings.notifications.systemPane", "नोटिफ़िकेशन", "सूचनाएँ"],
  ["ja", "settings.background.label", "ウインドウを閉じても", "ウィンドウを閉じても"],
  ["zh-Hans", "settings.background.description.on", "点按菜单栏图标即可重新打开窗口。", "单击通知区域图标即可重新打开窗口。"],
  ["zh-Hant", "settings.background.description.on", "點按選單列圖示即可重新開啟視窗。", "按一下通知區域圖示即可重新開啟視窗。"],
  ["ko", "settings.background.label", "윈도우를 닫아도 백그라운드에서 계속 실행", "창을 닫아도 백그라운드에서 계속 실행"],
  ["ko", "connection.incompatible.body", "이 윈도우와 연결된", "이 창과 연결된"],
  ["ko", "menu.closeWindow", "윈도우 닫기", "창 닫기"],
] as const)("words %s %s for each host, the macOS text as before", (language, key, mac, windows) => {
  announceHost(MAC_HOST);
  expect(format(language, key)).toContain(mac);
  announceHost(WINDOWS_HOST);
  expect(format(language, key)).toContain(windows);
});

it("asks Japanese and Korean Windows readers to change a setting in Settings rather than set it, and keeps the Mitteilungen the German section is named for", () => {
  announceHost(MAC_HOST);
  expect(format("ja", "settings.notifications.managedBySystem")).toContain("システム設定 で設定してください。");
  expect(format("ko", "settings.notifications.managedBySystem")).toContain("시스템 설정에서 설정해 주세요.");
  expect(format("de", "settings.notifications.managedBySystem")).toBe(
    "macOS verwaltet diese Mitteilungen. Deaktiviere sie oder ändere ihre Darstellung in den Systemeinstellungen. Es gibt keinen eigenen Schalter in der App.",
  );
  announceHost(WINDOWS_HOST);
  expect(format("ja", "settings.notifications.managedBySystem")).toContain("設定 で変更してください。");
  expect(format("ko", "settings.notifications.managedBySystem")).toContain("설정에서 변경해 주세요.");
  // The notifications are named as the section heading names them; only the pane's own name is Windows' word.
  expect(format("de", "settings.notifications.managedBySystem")).toContain("Windows verwaltet diese Mitteilungen.");
  expect(format("de", "settings.notifications.systemPane")).toBe("Benachrichtigungen");
});

it("fixes the shared wording on every host", () => {
  for (const host of [MAC_HOST, WINDOWS_HOST]) {
    announceHost(host);
    // Portuguese: the settings app's name is plural on both systems.
    expect(messageText("pt", "settings.notifications.openFailed")).toContain("{systemSettings} não abriram.");
    expect(messageText("pt", "settings.permissions.openFailed")).toContain("{systemSettings} não abriram.");
    // Korean: a particle after a placeholder gives both forms.
    expect(format("ko", "dialog.removeProject.trash")).toMatch(/휴지통\(으\)로 이동$/);
    expect(messageText("ko", "settings.background.description.on")).toContain("{menuBarIcon}(으)로");
  }
  for (const form of Object.values(CATALOGS.ko ?? {}).flatMap(formsOf)) expect(form).not.toMatch(/\{\w+\}(으로|을|를|은|는|이|가|과|와)(?!\()/);
  // The account the daemon runs as is named once, and what it must read is the folder.
  expect(messageText("en", "daemon.directory_unreadable.accountPermissions")).toBe(
    "{path} could not be read. On {osName}, the daemon reads folders with the permissions of the user account it runs as. Check that this account can read the folder, then try again: {detail}",
  );
  expect(messageText("en", "daemon.directory_unreadable.accountPermissions.recovery")).toBe("Check that the user account the daemon runs as can read the folder, then try again.");
});
