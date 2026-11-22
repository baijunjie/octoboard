import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { installerLayout } from "../../../scripts/fetch-conpty.mjs";

const windowsConfigUrl = new URL("../src-tauri/tauri.windows.conf.json", import.meta.url);
const baseConfigUrl = new URL("../src-tauri/tauri.conf.json", import.meta.url);
const hooksUrl = new URL("../src-tauri/windows/installer-hooks.nsh", import.meta.url);
const windowsConfig = JSON.parse(readFileSync(windowsConfigUrl, "utf8"));
const baseConfig = JSON.parse(readFileSync(baseConfigUrl, "utf8"));
// A Windows checkout may have turned the line endings into CRLF.
const hooks = readFileSync(hooksUrl, "utf8").replaceAll("\r\n", "\n");

// The files the installer ships but the main binary and the uninstaller, as `$INSTDIR`-relative paths with
// slashes: the daemon from `externalBin` (installed under its bare name, with `.exe`) and every resource target.
// The platform config is a merge patch over the base one: objects merge key by key, arrays replace.
function shippedFiles() {
  const externalBin = windowsConfig.bundle?.externalBin ?? baseConfig.bundle?.externalBin ?? [];
  const daemons = externalBin.map((entry) => `${path.posix.basename(entry)}.exe`);
  const resources = Object.values({ ...baseConfig.bundle?.resources, ...windowsConfig.bundle?.resources });
  return [...daemons, ...resources];
}

// The files the hooks hand to each action: the `$INSTDIR\...` string turned back to slashes, and the number
// that names where the set-aside of the file is recorded.
function listedFiles() {
  const body = /!macro OCTOBOARD_FOR_EACH_SHIPPED_FILE action\n([\s\S]*?)!macroend/.exec(hooks);
  assert.ok(body, "the hooks define the macro that lists the shipped files");
  return [...body[1].matchAll(/^\s*!insertmacro \$\{action\} "\$INSTDIR\\([^"]+)" (\d+)\s*$/gm)].map((match) => ({
    file: match[1].replaceAll("\\", "/"),
    number: Number(match[2]),
  }));
}

// The shell's own source, for what the hooks have to name as the shell does.
const loginEntryUrl = new URL("../src-tauri/src/launch_at_login/windows.rs", import.meta.url);
const jumpListUrl = new URL("../src-tauri/src/menu/jump_list.rs", import.meta.url);
const cleanUpUrl = new URL("../src-tauri/src/system_changes/headless.rs", import.meta.url);
const defined = (name) => new RegExp(`^!define ${name} "([^"\\n]*)"\\s*$`, "m").exec(hooks)?.[1];

// The hooks' language strings: name, then the `OCTOBOARD_LANG_<NAME>` suffix of the language, then the text.
const langStrings = [...hooks.matchAll(/^LangString (\w+) \$\{OCTOBOARD_LANG_(\w+)\} "([^"\n]*)"\s*$/gm)].map(
  ([, name, language, text]) => ({ name, language, text }),
);
const langStringNames = [...new Set(langStrings.map(({ name }) => name))];
// One string in every language, keyed by the language's NSIS name in upper case.
const translations = (name) => new Map(langStrings.filter((entry) => entry.name === name).map((entry) => [entry.language, entry.text]));

const macroBody = (name) => new RegExp(`^!macro ${name}(?: [^\\n]*)?\\n([\\s\\S]*?)^!macroend`, "m").exec(hooks)?.[1];

test("the Windows config ships the installer hooks, and the file is there", () => {
  const named = windowsConfig.bundle.windows.nsis.installerHooks;
  assert.equal(path.resolve(path.dirname(fileURLToPath(windowsConfigUrl)), named), fileURLToPath(hooksUrl));
  assert.ok(existsSync(hooksUrl), `${named} exists`);
});

test("the hooks define the pre-install, post-install and pre-uninstall hooks of Tauri's installer", () => {
  for (const hook of ["NSIS_HOOK_PREINSTALL", "NSIS_HOOK_POSTINSTALL", "NSIS_HOOK_PREUNINSTALL"]) {
    assert.match(hooks, new RegExp(`^!macro ${hook}\\s*$`, "m"), `${hook} is defined`);
  }
});

test("the hooks list exactly the files the installer ships but the main binary and the uninstaller", () => {
  const shipped = shippedFiles();
  assert.ok(shipped.length > 1, "the config ships the daemon and resources");
  assert.deepEqual(listedFiles().map(({ file }) => file).sort(), [...shipped].sort());
});

test("every ConPTY file either architecture's installer layout fetches is shipped, and so listed in the hooks", () => {
  const listed = listedFiles().map(({ file }) => file);
  for (const arch of ["x64", "arm64"]) {
    for (const { target } of installerLayout(arch)) assert.ok(listed.includes(target), `${target} is listed`);
  }
});

test("each listed file has its own number, and a variable declared for what is moved aside from it", () => {
  const numbers = listedFiles().map(({ number }) => number);
  assert.ok(numbers.length > 0);
  assert.equal(new Set(numbers).size, numbers.length, `the numbers ${numbers.join(", ")} are distinct`);
  const declared = [...hooks.matchAll(/^Var OctoboardSetAside(\d+)\s*$/gm)].map((match) => Number(match[1]));
  for (const number of numbers) assert.ok(declared.includes(number), `Var OctoboardSetAside${number} is declared`);
});

test("the pre-install hook ends the running application first, then moves aside over the list", () => {
  const preInstall = macroBody("NSIS_HOOK_PREINSTALL") ?? "";
  const check = preInstall.search(/!insertmacro CheckIfAppIsRunning "\$INSTDIR\\\$\{MAINBINARYNAME\}\.exe"/);
  const setAside = preInstall.search(/!insertmacro OCTOBOARD_FOR_EACH_SHIPPED_FILE OCTOBOARD_SET_ASIDE_IF_IN_USE/);
  assert.notEqual(check, -1, "the check of the main binary runs");
  assert.notEqual(setAside, -1, "the set-aside runs over the list");
  assert.ok(check < setAside, "the check runs before anything is moved aside");
});

test("the post-install hook puts back what a failed write left missing, then deletes what was moved aside", () => {
  const postInstall = macroBody("NSIS_HOOK_POSTINSTALL") ?? "";
  const putBack = postInstall.search(/!insertmacro OCTOBOARD_FOR_EACH_SHIPPED_FILE OCTOBOARD_PUT_BACK\s*$/m);
  const deletion = postInstall.search(/!insertmacro OCTOBOARD_FOR_EACH_SHIPPED_FILE OCTOBOARD_DELETE_SET_ASIDE\s*$/m);
  assert.notEqual(putBack, -1, "the put-back runs over the list");
  assert.notEqual(deletion, -1, "the delete runs over the list");
  assert.ok(putBack < deletion, "a file is put back before what was moved aside is deleted");
});

test("deleting what was moved aside is a plain delete, without asking for a reboot", () => {
  const deletion = macroBody("OCTOBOARD_DELETE_SET_ASIDE");
  assert.match(deletion ?? "", /^\s*Delete "\$\{path\}\$\{OCTOBOARD_SET_ASIDE\}\*"\s*$/m);
  assert.doesNotMatch(deletion ?? "", /REBOOTOK/i, "a reboot flag would make the finish page offer a reboot");
});

test("the pre-uninstall hook deletes what was moved aside, and what is still held at the next reboot", () => {
  assert.match(macroBody("NSIS_HOOK_PREUNINSTALL") ?? "", /OCTOBOARD_FOR_EACH_SHIPPED_FILE OCTOBOARD_DELETE_SET_ASIDE_AT_REBOOT\s*$/m);
  assert.match(macroBody("OCTOBOARD_DELETE_SET_ASIDE_AT_REBOOT") ?? "", /^\s*Delete \/REBOOTOK "\$\{path\}\$\{OCTOBOARD_SET_ASIDE\}\*"\s*$/m);
});

test("a failed install puts back what it moved aside, over the same list", () => {
  const onFailed = /^Function \.onInstFailed\n([\s\S]*?)^FunctionEnd/m.exec(hooks)?.[1];
  assert.ok(onFailed, ".onInstFailed is defined");
  assert.match(onFailed, /!insertmacro OCTOBOARD_FOR_EACH_SHIPPED_FILE OCTOBOARD_PUT_BACK\s*$/m);
});

test("the uninstaller removes what Task Manager recorded about the login entry, which the template's own leaves", () => {
  const loginEntry = readFileSync(loginEntryUrl, "utf8");
  const key = (name) => new RegExp(`pub const ${name}: &str =\\s*r"([^"]+)";`).exec(loginEntry)?.[1];
  assert.ok(key("APPROVED_KEY"), "the shell names the key");
  assert.equal(defined("OCTOBOARD_STARTUP_APPROVED_KEY"), key("APPROVED_KEY"));
  // The shell names the value after the product (`menu::APP_NAME`), as the installer's ${PRODUCTNAME} is.
  assert.match(loginEntry, /let name = crate::menu::APP_NAME;/);
  const removal = macroBody("OCTOBOARD_REMOVE_FEATURE_ENTRIES") ?? "";
  assert.match(removal, /^\s*DeleteRegValue HKCU "\$\{OCTOBOARD_STARTUP_APPROVED_KEY\}" "\$\{PRODUCTNAME\}"\s*$/m);
  // The `Run` value itself is the template's to remove; the hooks do not repeat it.
  assert.doesNotMatch(removal, /CurrentVersion\\Run"/);
  const lines = removal.split("\n").map((line) => line.trim()).filter((line) => line !== "" && !line.startsWith(";"));
  assert.equal(lines[0], "${If} $UpdateMode <> 1", "as the template guards its own removal of the entry");
  assert.equal(lines.at(-1), "${EndIf}");
  assert.equal(lines.filter((line) => line.startsWith("${If} $UpdateMode")).length, 1);
});

test("the uninstaller has the application remove its jump list, by the argument the shell reads and the bundle's identifier, while the executable is there", () => {
  const argument = /pub const DELETE_ARGUMENT: &str = "([^"]+)";/.exec(readFileSync(jumpListUrl, "utf8"))?.[1];
  assert.ok(argument, "the shell names the argument");
  assert.equal(defined("OCTOBOARD_DELETE_JUMP_LIST_ARGUMENT"), argument);
  const removal = macroBody("OCTOBOARD_REMOVE_FEATURE_ENTRIES") ?? "";
  assert.match(removal, /^\s*ExecWait '"\$INSTDIR\\\$\{MAINBINARYNAME\}\.exe" \$\{OCTOBOARD_DELETE_JUMP_LIST_ARGUMENT\} \$\{BUNDLEID\}'\s*$/m);
  // Before anything of the installation is deleted: the hook is the pre-uninstall one, and this runs first in it.
  const preUninstall = macroBody("NSIS_HOOK_PREUNINSTALL") ?? "";
  const entries = preUninstall.search(/!insertmacro OCTOBOARD_REMOVE_FEATURE_ENTRIES\s*$/m);
  const setAside = preUninstall.search(/!insertmacro OCTOBOARD_FOR_EACH_SHIPPED_FILE/);
  assert.notEqual(entries, -1, "the pre-uninstall hook removes the entries");
  assert.ok(entries < setAside);
});

// The lines of a macro's body that do something: no blank ones, no comments.
const steps = (body) => body.split("\n").map((line) => line.trim()).filter((line) => line !== "" && !line.startsWith(";"));

test("the uninstaller has the application run the clean-up, by the argument and the status the shell names, into a file in the installation's folder that it deletes", () => {
  const cleanUp = readFileSync(cleanUpUrl, "utf8");
  const argument = /pub const ARGUMENT: &str = "([^"]+)";/.exec(cleanUp)?.[1];
  assert.ok(argument, "the shell names the argument");
  assert.equal(defined("OCTOBOARD_CLEAN_UP_ARGUMENT"), argument);
  const left = /^\s*Left = (\d+),$/m.exec(cleanUp)?.[1];
  assert.ok(left, "the shell names the status of a clean-up that left something");
  assert.equal(/^!define OCTOBOARD_CLEAN_UP_LEFT (\d+)\s*$/m.exec(hooks)?.[1], left);
  assert.match(cleanUp, /^\s*Cleared = 0,$/m, "nothing left is the status the hooks show nothing for");
  const body = macroBody("OCTOBOARD_CLEAN_UP") ?? "";
  assert.match(body, /^\s*ExecWait '"\$INSTDIR\\\$\{MAINBINARYNAME\}\.exe" \$\{OCTOBOARD_CLEAN_UP_ARGUMENT\} "\$INSTDIR\\uninstall-clean-up\.txt" \$\(OctoboardLanguageTag\)' \$0\s*$/m);
  // What it left is read as the shell writes it, and only for that status; any other but 0 is a clean-up that did not run.
  assert.match(cleanUp, /encode_utf16\(\)\s*\.flat_map\(u16::to_le_bytes\)/);
  assert.match(body, /\$\{ElseIf\} \$0 = \$\{OCTOBOARD_CLEAN_UP_LEFT\}[\s\S]*FileOpen \$1 "\$INSTDIR\\uninstall-clean-up\.txt" r[\s\S]*FileReadUTF16LE \$1 \$2[\s\S]*Delete "\$INSTDIR\\uninstall-clean-up\.txt"[\s\S]*\n\s*\$\{ElseIf\} \$0 <> 0\s*\n\s*StrCpy \$OctoboardCleanUpLeft "\$\(OctoboardCleanUpNotRun\)\$\(OctoboardCleanUpLogHint\)"/);
  // The hint of where a list may be saved goes only with a clean-up that was started: one that could not be
  // started (`ExecWait` failed) and one whose executable is missing ran nothing, and saved none.
  const lines = steps(body);
  const plain = 'StrCpy $OctoboardCleanUpLeft "$(OctoboardCleanUpNotRun)"';
  assert.equal(lines[lines.indexOf("${If} ${Errors}") + 1], plain);
  assert.equal(lines[lines.indexOf("${ElseIf} $EXEDIR != $INSTDIR") + 1], plain);
  assert.equal(lines.filter((line) => line === plain).length, 2);
  assert.equal(lines.filter((line) => line.includes("$(OctoboardCleanUpLogHint)")).length, 2);
});

test("a clean-up that says it left something and whose list cannot be read is told as one that did not run, with where the list is saved", () => {
  const lines = steps(macroBody("OCTOBOARD_CLEAN_UP") ?? "");
  const deleted = lines.lastIndexOf('Delete "$INSTDIR\\uninstall-clean-up.txt"');
  assert.notEqual(deleted, -1);
  // The heading goes before the list only where a list was read: the status alone is not taken for one.
  assert.deepEqual(lines.slice(deleted + 1, deleted + 7), [
    '${If} $OctoboardCleanUpLeft == ""',
    'StrCpy $OctoboardCleanUpLeft "$(OctoboardCleanUpNotRun)$(OctoboardCleanUpLogHint)"',
    "${Else}",
    "StrCpy $OctoboardCleanUpEnded 1",
    'StrCpy $OctoboardCleanUpLeft "$(OctoboardCleanUpLeftHeading)$\\r$\\n$\\r$\\n$OctoboardCleanUpLeft"',
    "${EndIf}",
  ]);
  assert.equal(lines.filter((line) => line.includes("$(OctoboardCleanUpLeftHeading)")).length, 1);
  // The hint names the log the shell writes, in the data directory's `logs`.
  const cleanUp = readFileSync(cleanUpUrl, "utf8");
  const log = /const LOG_NAME: &str = "([^"]+)";/.exec(cleanUp)?.[1];
  assert.ok(log, "the shell names the log");
  assert.match(cleanUp, /crate::diagnostics::logs_dir\(\)\.map\(\|dir\| dir\.join\(LOG_NAME\)\)/);
  const hints = translations("OctoboardCleanUpLogHint");
  assert.ok(hints.size > 0);
  for (const [language, hint] of hints) {
    assert.ok(hint.includes(`$PROFILE\\.octoboard\\logs\\${log}`), `${language} names the log`);
  }
  assert.ok(hints.get("ENGLISH").endsWith(` $PROFILE\\.octoboard\\logs\\${log}.`));
});

test("the uninstaller's heading takes no more of its message box than the shell leaves for it", () => {
  const cleanUp = readFileSync(cleanUpUrl, "utf8");
  const units = (name) => Number(new RegExp(`const ${name}: usize = (\\d+);`).exec(cleanUp)?.[1]);
  assert.ok(units("HEADING_UNITS") > 0 && units("MESSAGE_UNITS") > 0);
  assert.match(cleanUp, /const SHOWN_LIMIT: usize = MESSAGE_UNITS - HEADING_UNITS;/);
  // In every language (a JavaScript string's length counts UTF-16 units, as NSIS's Unicode strings do): the
  // heading, then the two line ends before the list.
  const headings = translations("OctoboardCleanUpLeftHeading");
  assert.ok(headings.size > 0);
  for (const [language, heading] of headings) {
    assert.ok(heading.length + 4 <= units("HEADING_UNITS"), `${language}: ${heading.length} units`);
  }
  // An NSIS string, the message being one, holds 1024 with its end.
  assert.equal(units("MESSAGE_UNITS"), 1023);
  // The messages that carry no list are whole as well.
  const hints = translations("OctoboardCleanUpLogHint");
  for (const [language, notRun] of translations("OctoboardCleanUpNotRun")) {
    const longest = `${notRun}${hints.get(language)}`;
    assert.ok(longest.length + 260 <= units("MESSAGE_UNITS"), `${language}, with a profile folder as long as a path may be`);
  }
});

test("the clean-up runs only in a real uninstall: not for an update, and not in the uninstaller a setup runs in place", () => {
  const lines = steps(macroBody("OCTOBOARD_CLEAN_UP") ?? "");
  // Everything but the reset of what is told and of whether it ended sits under the one guard, which is tested before the executable is
  // looked for: an upgrade is never told that nothing was checked.
  assert.deepEqual(lines.slice(0, 5), [
    'StrCpy $OctoboardCleanUpLeft ""',
    "StrCpy $OctoboardCleanUpEnded 0",
    "${If} $UpdateMode <> 1",
    // `_?=` keeps the uninstaller beside the executable; a real uninstall runs from a copy in the temporary folder.
    '${AndIfNot} ${FileExists} "$EXEDIR\\${MAINBINARYNAME}.exe"',
    '${If} ${FileExists} "$INSTDIR\\${MAINBINARYNAME}.exe"',
  ]);
  assert.equal(lines.at(-1), "${EndIf}");
  const depth = lines.slice(4, -1).reduce((open, line) => {
    assert.ok(open > 0, `"${line}" is under the guard`);
    return open + (/^\$\{If(Not)?\} /.test(line) ? 1 : 0) - (line === "${EndIf}" ? 1 : 0);
  }, 1);
  assert.equal(depth, 1);
  // The application is ended before it is started for the clean-up, and the user is shown what is being done.
  const at = (pattern) => lines.findIndex((line) => pattern.test(line));
  const ended = at(/^!insertmacro CheckIfAppIsRunning "\$INSTDIR\\\$\{MAINBINARYNAME\}\.exe"/);
  const shown = at(/^DetailPrint "\$\(OctoboardCleanUpDetail\)"$/);
  const run = at(/^ExecWait /);
  assert.ok(ended !== -1 && shown !== -1 && ended < run && shown < run);
});

test("a real uninstall with no executable to run says that nothing was checked, and an uninstaller run in place says nothing", () => {
  const lines = steps(macroBody("OCTOBOARD_CLEAN_UP") ?? "");
  // The last branch of the look for the executable, under the guard of a real uninstall. An uninstaller run with
  // `_?=` runs from the installation's own folder, so `$EXEDIR` is `$INSTDIR` there.
  assert.deepEqual(lines.slice(-4), [
    "${ElseIf} $EXEDIR != $INSTDIR",
    'StrCpy $OctoboardCleanUpLeft "$(OctoboardCleanUpNotRun)"',
    "${EndIf}",
    "${EndIf}",
  ]);
});

// The NSIS template the installed Tauri CLI builds the installer from, which the CLI's native binding holds as
// text: `undefined` where no binding for this machine is installed.
function tauriTemplate() {
  const require = createRequire(import.meta.url);
  const cli = require.resolve("@tauri-apps/cli/package.json");
  const fromCli = createRequire(cli);
  for (const name of Object.keys(JSON.parse(readFileSync(cli, "utf8")).optionalDependencies ?? {})) {
    let manifest;
    try {
      manifest = fromCli.resolve(`${name}/package.json`);
    } catch {
      continue;
    }
    const binding = path.join(path.dirname(manifest), JSON.parse(readFileSync(manifest, "utf8")).main);
    return readFileSync(binding, "latin1");
  }
  return undefined;
}

test("Tauri's template starts the installed uninstaller in place, with `_?=`, from its reinstall page alone, and never in update mode", () => {
  const template = tauriTemplate();
  assert.ok(template, "a native binding of @tauri-apps/cli is installed");
  const leave = /\nFunction PageLeaveReinstall\n([\s\S]*?)\nFunctionEnd\n/.exec(template)?.[1];
  assert.ok(leave, "the template has the function that leaves its reinstall page");
  // What the clean-up's guard rests on: the uninstaller a setup runs is given `_?=` with the installation's folder,
  // so it runs in place, beside the application's executable.
  assert.match(leave, /\n\s*ReadRegStr \$R1 SHCTX "\$\{UNINSTKEY\}" "UninstallString"\n[\s\S]*?\n\s*StrCpy \$R1 "\$R1 _\?=\$4"[^\n]*\n\s*ExecWait '\$R1' \$0\n/);
  assert.equal(template.split("_?=").length - 1, 1, "nothing else in the template starts an uninstaller in place");
  // And nothing else starts the installed uninstaller at all: its command is read from the registry and run on that
  // page alone (the other read and run there are the earlier WiX installer's, from another key), and no command
  // anywhere names `uninstall.exe` to run it.
  const count = (text, part) => text.split(part).length - 1;
  assert.equal(count(leave, 'ReadRegStr $R1 SHCTX "${UNINSTKEY}" "UninstallString"'), 1);
  assert.equal(count(leave, "ExecWait '$R1'"), 2, "the installed NSIS uninstaller's run and the WiX one's");
  assert.equal(count(template, "ExecWait '$R1'"), 2, "no run of a command read that way outside the page");
  assert.doesNotMatch(template, /\n[^\n;]*\bExec(Wait|Shell|ShellWait)?\b[^\n]*uninstall\.exe/i);
  // The in-app update's setup leaves the page without uninstalling.
  assert.match(leave, /\$\{If\} \$UpdateMode = 1\n\s*Goto reinst_done\n/);
  // The same version, an upgrade and a downgrade all reach that one uninstall.
  for (const version of ["$R0 = 0", "$R0 = 1", "$R0 = -1"]) {
    assert.match(leave, new RegExp(`If\\} ${version.replaceAll("$", "\\$")} ;[^\\n]*\\n[\\s\\S]*?Goto reinst_uninstall`));
  }
});

test("the clean-up comes first in the pre-uninstall hook, and nothing in the hooks stops the uninstall over it", () => {
  const lines = steps(macroBody("NSIS_HOOK_PREUNINSTALL") ?? "");
  assert.equal(lines[0], "!insertmacro OCTOBOARD_CLEAN_UP");
  assert.doesNotMatch(macroBody("OCTOBOARD_CLEAN_UP") ?? "", /^\s*(Abort|Quit)\b/m);
});

test("the uninstaller tells what the clean-up left at its end, and a silent one shows nothing", () => {
  assert.deepEqual(steps(macroBody("NSIS_HOOK_POSTUNINSTALL") ?? ""), [
    '${If} $OctoboardCleanUpLeft != ""',
    'MessageBox MB_OK|MB_ICONEXCLAMATION "$OctoboardCleanUpLeft" /SD IDOK',
    "${EndIf}",
  ]);
  assert.match(hooks, /^Var OctoboardCleanUpLeft\s*$/m);
  // The messages are written out in full: the template defines the product's name only after it reads this file.
  assert.deepEqual([...langStringNames].sort(), ["OctoboardCleanUpDetail", "OctoboardCleanUpLeftHeading", "OctoboardCleanUpLogHint", "OctoboardCleanUpNotRun", "OctoboardLanguageTag", "OctoboardReleaseDetail"]);
  for (const { name, language, text } of langStrings) assert.doesNotMatch(text, /\$\{/, `${name} in ${language}`);
});

// The NSIS language each UI catalog is shown in, by NSIS's name for it (`Contrib/Language files/<name>.nlf`) and the
// Windows language ID that file gives. Portuguese is the UI's Brazilian Portuguese.
const NSIS_LANGUAGES = {
  ar: ["Arabic", 1025],
  de: ["German", 1031],
  en: ["English", 1033],
  es: ["Spanish", 1034],
  fr: ["French", 1036],
  hi: ["Hindi", 1081],
  id: ["Indonesian", 1057],
  it: ["Italian", 1040],
  ja: ["Japanese", 1041],
  ko: ["Korean", 1042],
  pt: ["PortugueseBR", 1046],
  ru: ["Russian", 1049],
  th: ["Thai", 1054],
  tr: ["Turkish", 1055],
  vi: ["Vietnamese", 1066],
  "zh-CN": ["SimpChinese", 2052],
  "zh-TW": ["TradChinese", 1028],
};

// The UI's languages, as `languages.ts` lists them, which is also the set of catalogs under `messages/`.
function uiLanguages() {
  const list = /export const LANGUAGES = \[([\s\S]*?)\] as const;/.exec(readFileSync(new URL("../../../packages/ui/src/i18n/languages.ts", import.meta.url), "utf8"))?.[1];
  assert.ok(list, "languages.ts lists the languages");
  const languages = [...list.matchAll(/"([^"]+)"/g)].map((match) => match[1]);
  const catalogs = readdirSync(new URL("../../../packages/ui/src/i18n/messages/", import.meta.url)).filter((file) => file.endsWith(".ts")).map((file) => file.slice(0, -3));
  assert.deepEqual([...catalogs].sort(), [...languages].sort(), "each language has a catalog");
  return languages;
}

const nsis = windowsConfig.bundle.windows.nsis;

test("the installer is configured in the UI's languages, in the order of their tags, chosen from the system's language with no selector", () => {
  const languages = uiLanguages();
  assert.deepEqual(Object.keys(NSIS_LANGUAGES).sort(), [...languages].sort(), "every UI language has an NSIS language");
  const byTag = [...languages].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
  assert.deepEqual(nsis.languages, byTag.map((language) => NSIS_LANGUAGES[language][0]));
  assert.equal(nsis.displayLanguageSelector, false);
});

// The order NSIS keeps its language tables in: the order the script first names each language.
const tableOrder = () => [...new Set(langStrings.map(({ language }) => language))];

test("English is the language a system language matching none gets, and Simplified Chinese the one a Chinese of neither gets", () => {
  // NSIS takes the first table with the system's primary language, then the first of all. The hooks name every
  // language before the template loads any: English first, then the configured order.
  const configured = nsis.languages.map((name) => name.toUpperCase());
  assert.deepEqual(tableOrder(), ["ENGLISH", ...configured.filter((name) => name !== "ENGLISH")]);
  assert.ok(tableOrder().indexOf("SIMPCHINESE") < tableOrder().indexOf("TRADCHINESE"));
  // Nothing in the hooks before their strings names a language either.
  const first = hooks.search(/^LangString /m);
  assert.doesNotMatch(hooks.slice(0, first), /^\s*(LangString|LoadLanguageFile|!insertmacro MUI_LANGUAGE)\b/m);
  // Nor in the template before it reads the hooks.
  const template = tauriTemplate();
  assert.ok(template, "a native binding of @tauri-apps/cli is installed");
  const start = template.lastIndexOf("Unicode true", template.indexOf('!include "{{installer_hooks}}"'));
  const before = template.slice(start, template.indexOf('!include "{{installer_hooks}}"'));
  assert.ok(start !== -1 && before.length > 0, "the template reads the hooks");
  assert.doesNotMatch(before, /^\s*(LangString|LoadLanguageFile|!insertmacro MUI_LANGUAGE|Name|Caption)\b|\$\{LANG_/m);
  assert.ok(template.indexOf('!include "{{installer_hooks}}"') < template.indexOf('!insertmacro MUI_LANGUAGE "{{this}}"'));
});

test("the uninstaller passes the clean-up the tag of the language it shows", () => {
  const tags = translations("OctoboardLanguageTag");
  const expected = new Map(Object.entries(NSIS_LANGUAGES).map(([tag, [name]]) => [name.toUpperCase(), tag]));
  assert.deepEqual(new Map([...tags].sort()), new Map([...expected].filter(([name]) => tags.has(name)).sort()));
  assert.equal(tags.size, nsis.languages.length);
});

test("the installer saves its language where the template's uninstaller reads it, also after a silent install", () => {
  const template = tauriTemplate();
  assert.ok(template, "a native binding of @tauri-apps/cli is installed");
  for (const name of ["MUI_LANGDLL_REGISTRY_ROOT", "MUI_LANGDLL_REGISTRY_KEY", "MUI_LANGDLL_REGISTRY_VALUENAME"]) {
    assert.match(template, new RegExp(`\\n!define ${name} "[^"]+"\\n`), `the template defines ${name}`);
  }
  const unInit = /\nFunction un\.onInit\n([\s\S]*?)\nFunctionEnd\n/.exec(template)?.[1];
  assert.match(unInit ?? "", /!insertmacro MUI_UNGETLANGUAGE/);
  assert.match(macroBody("NSIS_HOOK_POSTINSTALL") ?? "", /^\s*WriteRegStr \$\{MUI_LANGDLL_REGISTRY_ROOT\} "\$\{MUI_LANGDLL_REGISTRY_KEY\}" "\$\{MUI_LANGDLL_REGISTRY_VALUENAME\}" \$LANGUAGE\s*$/m);
});

test("the hooks name each configured language by its Windows language ID, and no other", () => {
  const ids = Object.fromEntries([...hooks.matchAll(/^!define OCTOBOARD_LANG_(\w+) (\d+)\s*$/gm)].map((match) => [match[1], Number(match[2])]));
  const expected = Object.fromEntries(Object.values(NSIS_LANGUAGES).filter(([name]) => nsis.languages.includes(name)).map(([name, id]) => [name.toUpperCase(), id]));
  assert.deepEqual(ids, expected);
});

// The text's parameters: variables, defines, NSIS escapes and the template's `{{product_name}}`, which Tauri's own files
// use for the product's name.
const parameters = (text) =>
  [...text.replaceAll("{{product_name}}", "${PRODUCTNAME}").matchAll(/\$\{\w+\}|\$\\.|\$\w+/g)].map((match) => match[0]).sort();

test("every hook string is set once in every configured language, with the English one's parameters", () => {
  const configured = nsis.languages.map((name) => name.toUpperCase());
  const used = [...hooks.matchAll(/\$\((\w+)\)/g)].map((match) => match[1]);
  assert.ok(used.length > 0);
  for (const name of used) assert.ok(langStringNames.includes(name), `$(${name}) is set`);
  for (const name of langStringNames) {
    assert.ok(used.includes(name), `${name} is used`);
    const entries = langStrings.filter((entry) => entry.name === name);
    assert.deepEqual(entries.map(({ language }) => language).sort(), [...configured].sort(), `${name} in each language once`);
    const english = parameters(translations(name).get("ENGLISH"));
    for (const { language, text } of entries) {
      assert.deepEqual(parameters(text), english, `${name} in ${language}`);
      assert.notEqual(text.trim(), "", `${name} in ${language}`);
    }
  }
});

// Tauri's own language files, which the CLI's native binding holds as text: the languages it translates, and the keys
// and texts of its English file.
function tauriLanguages(template) {
  const languages = new Set([...template.matchAll(/^LangString \w+ \$\{LANG_(\w+)\} "/gm)].map((match) => match[1]));
  const english = Object.fromEntries([...template.matchAll(/^LangString (\w+) \$\{LANG_ENGLISH\} "([^"\n]*)"/gm)].map((match) => [match[1], match[2]]));
  return { languages, english };
}

test("a configured language Tauri ships no strings for has its own file of all of Tauri's strings, and only such a one", () => {
  const template = tauriTemplate();
  assert.ok(template, "a native binding of @tauri-apps/cli is installed");
  const { languages, english } = tauriLanguages(template);
  assert.ok(languages.has("ENGLISH") && Object.keys(english).length > 10);
  const missing = nsis.languages.filter((name) => !languages.has(name.toUpperCase()));
  assert.deepEqual(Object.keys(nsis.customLanguageFiles ?? {}).sort(), missing.sort());
  for (const [name, file] of Object.entries(nsis.customLanguageFiles ?? {})) {
    const url = new URL(`../src-tauri/${file}`, import.meta.url);
    const raw = readFileSync(url);
    // Tauri writes the file out for makensis as a UTF-8 byte order mark followed by the file's own bytes
    // (`write_utf8_with_bom` in tauri-bundler's `nsis/mod.rs`), so that NSIS reads it as UTF-8 whatever the code page of
    // the machine that builds the installer. A source file with a mark of its own comes out with two, and makensis
    // stops on the first line; the copy Tauri writes has exactly one.
    assert.notDeepEqual([...raw.subarray(0, 3)], [0xef, 0xbb, 0xbf], `${file} carries no byte order mark of its own`);
    const generated = Buffer.concat([Buffer.from([0xef, 0xbb, 0xbf]), raw]);
    assert.equal(generated.toString("latin1").split("\xef\xbb\xbf").length - 1, 1, `the copy Tauri writes of ${file} has one mark`);
    assert.match(new TextDecoder("utf-8", { fatal: true }).decode(raw), /^; /, `${file} is UTF-8 and starts with its comment`);
    const entries = [...raw.toString("utf8").matchAll(/^LangString (\w+) \$\{LANG_(\w+)\} "([^"\n]*)"\s*$/gm)];
    assert.ok(entries.every((match) => match[2] === name.toUpperCase()), `${file} sets ${name} only`);
    assert.deepEqual(entries.map((match) => match[1]).sort(), Object.keys(english).sort(), `${file} sets each of Tauri's keys`);
    for (const [, key, , text] of entries) {
      assert.deepEqual(parameters(text), parameters(english[key]), `${key} in ${file}`);
      // Tauri does not fill a custom file's template placeholders.
      assert.doesNotMatch(text, /\{\{/, `${key} in ${file}`);
    }
  }
});

// The UI's catalog of a language, as text.
const uiCatalog = (tag) => readFileSync(new URL(`../../../packages/ui/src/i18n/messages/${tag}.ts`, import.meta.url), "utf8");

test("the two sentences of a message with the log's place are joined as the UI joins sentences, with nothing written between them", () => {
  const lines = steps(macroBody("OCTOBOARD_CLEAN_UP") ?? "");
  const joined = lines.filter((line) => line.includes("$(OctoboardCleanUpLogHint)"));
  assert.equal(joined.length, 2);
  for (const line of joined) {
    assert.equal(line, 'StrCpy $OctoboardCleanUpLeft "$(OctoboardCleanUpNotRun)$(OctoboardCleanUpLogHint)"');
  }
  // The second sentence starts with the separator itself: NSIS takes an empty string for one not set.
  const hints = translations("OctoboardCleanUpLogHint");
  for (const [tag, [name]] of Object.entries(NSIS_LANGUAGES)) {
    const ui = /"common\.sentenceSeparator": "([^"]*)"/.exec(uiCatalog(tag))?.[1];
    assert.notEqual(ui, undefined, `the UI's ${tag} catalog has a sentence separator`);
    const hint = hints.get(name.toUpperCase());
    assert.ok(hint.startsWith(ui) && !/^\s/.test(hint.slice(ui.length)), tag);
  }
});

test("the progress lines of the hooks and of Tauri's strings the project writes end with an ellipsis character", () => {
  for (const name of ["OctoboardCleanUpDetail", "OctoboardReleaseDetail"]) {
    for (const [language, text] of translations(name)) assert.ok(text.endsWith("\u2026") && !text.endsWith("..."), `${name} in ${language}`);
  }
  for (const { name, language, text } of langStrings) assert.doesNotMatch(text, /\.\.\./, `${name} in ${language}`);
  for (const file of Object.values(nsis.customLanguageFiles ?? {})) {
    const text = readFileSync(new URL(`../src-tauri/${file}`, import.meta.url), "utf8");
    assert.doesNotMatch(text, /\.\.\./, file);
    for (const key of ["installingWebview2", "webview2Downloading"]) {
      assert.match(text, new RegExp(`^LangString ${key} \\$\\{LANG_\\w+\\} "[^"]*\u2026"`, "m"), `${key} in ${file}`);
    }
  }
});

test("in Arabic, the path in the hooks' messages is set off by left-to-right marks", () => {
  const lrm = "\u200e";
  for (const { name, text } of langStrings.filter((entry) => entry.language === "ARABIC")) {
    for (const match of text.matchAll(/\$PROFILE[^\s"]*?\.log/g)) {
      assert.equal(text[match.index - 1], lrm, `${name}: before the path`);
      assert.equal(text[match.index + match[0].length], lrm, `${name}: after the path`);
    }
  }
  assert.ok(translations("OctoboardCleanUpLogHint").get("ARABIC").includes(`${lrm}$PROFILE`));
});

test("the hooks are read as UTF-8 whatever the code page of the machine that builds the installer", () => {
  // Unlike a custom language file, Tauri does not copy the hooks: its template includes them where they are, by the
  // path it resolves, so their own mark is the only one.
  const template = tauriTemplate();
  assert.ok(template, "a native binding of @tauri-apps/cli is installed");
  assert.match(template, /\n!include "\{\{installer_hooks\}\}"\n/);
  const raw = readFileSync(hooksUrl);
  assert.deepEqual([...raw.subarray(0, 3)], [0xef, 0xbb, 0xbf]);
  assert.notDeepEqual([...raw.subarray(3, 6)], [0xef, 0xbb, 0xbf]);
});

test("the pre-uninstall hook ends with the wait for Windows to let go of the executable", () => {
  const lines = steps(macroBody("NSIS_HOOK_PREUNINSTALL") ?? "");
  assert.equal(lines.at(-1), "!insertmacro OCTOBOARD_WAIT_FOR_RELEASE", "after every run of the executable the hook starts");
  assert.equal(lines.filter((line) => line.includes("OCTOBOARD_WAIT_FOR_RELEASE")).length, 1);
});

test("the wait asks Restart Manager about the main binary as Tauri's check does, and is bounded to about 10 s", () => {
  const lines = steps(macroBody("OCTOBOARD_WAIT_FOR_RELEASE") ?? "");
  assert.ok(lines.includes('!insertmacro RestartManager_RegisterFile $R0 "$INSTDIR\\${MAINBINARYNAME}.exe"'));
  assert.ok(lines.includes("System::Call 'RSTRTMGR::RmGetList(p R0, *i .r1, *i .r2, p 0, *i .r3) i .r0'"));
  assert.ok(lines.includes("!insertmacro RestartManager_EndSession $R0"));
  // It waits only while the answer is the one the check prompts for, and gives up after the bound.
  const at = (line) => lines.indexOf(line);
  assert.equal(lines[at("${If} $0 <> ${ERROR_MORE_DATA}") + 1], "${ExitDo}");
  assert.equal(lines[at("${If} $4 >= ${OCTOBOARD_RELEASE_POLLS}") + 1], "${ExitDo}");
  assert.ok(at("${If} $4 >= ${OCTOBOARD_RELEASE_POLLS}") < at("Sleep ${OCTOBOARD_RELEASE_POLL_MS}"), "no wait after the last question");
  // The details say so once, when it first finds the executable held, before it waits: not on every uninstall.
  assert.deepEqual(lines.slice(at("${If} $4 = 0"), at("${If} $4 = 0") + 3), ["${If} $4 = 0", 'DetailPrint "$(OctoboardReleaseDetail)"', "${EndIf}"]);
  assert.ok(at("${If} $0 <> ${ERROR_MORE_DATA}") < at("${If} $4 = 0") && at("${If} $4 = 0") < at("IntOp $4 $4 + 1"));
  const value = (name) => Number(new RegExp(`^!define ${name} (\\d+)\\s*$`, "m").exec(hooks)?.[1]);
  const bound = value("OCTOBOARD_RELEASE_POLLS") * value("OCTOBOARD_RELEASE_POLL_MS");
  assert.ok(bound >= 5000 && bound <= 15000, `${bound} ms is about 10 s`);
  assert.ok(value("OCTOBOARD_RELEASE_POLL_MS") <= 500);
  // Every register it writes (the session in $R0, the answer in $0, the counts in $1 to $3, its own count in $4) is
  // the template's as it was: pushed first and popped last, in reverse.
  const pushed = lines.filter((line) => line.startsWith("Push ")).map((line) => line.slice(5));
  const popped = lines.filter((line) => line.startsWith("Pop ")).map((line) => line.slice(4));
  assert.deepEqual(pushed, ["$0", "$1", "$2", "$3", "$4", "$R0"]);
  assert.deepEqual(popped, [...pushed].reverse());
  assert.deepEqual(lines.slice(0, pushed.length), pushed.map((register) => `Push ${register}`));
  assert.deepEqual(lines.slice(-popped.length), popped.map((register) => `Pop ${register}`));
});

test("Tauri's uninstaller checks the running application right after the pre-uninstall hook, then deletes the main binary, with nothing between", () => {
  const template = tauriTemplate();
  assert.ok(template, "a native binding of @tauri-apps/cli is installed");
  const section = /\nSection Uninstall\n([\s\S]*?)\nSectionEnd\n/.exec(template)?.[1];
  assert.ok(section, "the template has the uninstall section");
  assert.deepEqual(steps(section).slice(0, 5), [
    "!ifmacrodef NSIS_HOOK_PREUNINSTALL",
    "!insertmacro NSIS_HOOK_PREUNINSTALL",
    "!endif",
    '!insertmacro CheckIfAppIsRunning "$INSTDIR\\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"',
    'Delete "$INSTDIR\\${MAINBINARYNAME}.exe"',
  ]);
  // The check prompts only where Restart Manager answers ERROR_MORE_DATA for the main binary, asked as the wait asks.
  const check = /\n!macro CheckIfAppIsRunning executablePath productName\n([\s\S]*?)\n!macroend\n/.exec(template)?.[1];
  assert.ok(check, "the template defines the check");
  const lines = steps(check);
  assert.ok(lines.includes('!insertmacro RestartManager_RegisterFile $R0 "${executablePath}"'));
  assert.ok(lines.includes("System::Call 'RSTRTMGR::RmGetList(p R0, *i .r1, *i .r2, p 0, *i .r3) i .r0'"));
  assert.equal(lines.filter((line) => line.includes("MessageBox")).length, 1);
  assert.ok(lines.indexOf("${If} $0 = ${ERROR_MORE_DATA}") < lines.findIndex((line) => line.includes("MessageBox")));
  assert.match(template, /\n!define \/ifndef ERROR_MORE_DATA 234\n/);
});

test("the jump list is removed by its own run of the executable only where the clean-up did not run to its end", () => {
  const removal = steps(macroBody("OCTOBOARD_REMOVE_FEATURE_ENTRIES") ?? "");
  const run = removal.findIndex((line) => line.startsWith("ExecWait ") && line.includes("${OCTOBOARD_DELETE_JUMP_LIST_ARGUMENT}"));
  assert.deepEqual(removal.slice(run - 2, run), ["${If} $OctoboardCleanUpEnded != 1", '${AndIf} ${FileExists} "$INSTDIR\\${MAINBINARYNAME}.exe"']);
  assert.match(hooks, /^Var OctoboardCleanUpEnded\s*$/m);
  // The clean-up says it ended only where it answered with a status of its own, which it sets only once the jump list
  // is removed (`settle`, whose order its own test holds): 0, or the status for what it left with a list read (a 1
  // with none is a process ended from outside); not where it could not be started or could not run.
  const cleanUp = readFileSync(cleanUpUrl, "utf8");
  assert.match(cleanUp, /settle\(\s*\|\| super::remove_jump_list\(&app\),[\s\S]*?\|status\| EXIT_STATUS\.store\(status, Ordering::SeqCst\),/);
  assert.match(cleanUp, /fn settle\([\s\S]*?\n\) \{\s*remove_jump_list\(\);\s*write\(\);\s*set\(status\.code\(\)\);/);
  const lines = steps(macroBody("OCTOBOARD_CLEAN_UP") ?? "");
  const ended = "StrCpy $OctoboardCleanUpEnded 1";
  assert.equal(lines.filter((line) => line === ended).length, 2);
  assert.equal(lines[lines.indexOf("${ElseIf} $0 = 0") + 1], ended);
  const left = lines.indexOf("${ElseIf} $0 = ${OCTOBOARD_CLEAN_UP_LEFT}");
  const listed = lines.indexOf('${If} $OctoboardCleanUpLeft == ""', left);
  assert.deepEqual(lines.slice(listed + 2, listed + 4), ["${Else}", ended], "only where a list was read");
  assert.equal(lines.indexOf(ended, left), listed + 3);
  assert.ok(lines.indexOf("${If} ${Errors}") < lines.indexOf("${ElseIf} $0 = 0"), "a run that could not be started is not one");
  // It runs before the jump list's removal.
  const preUninstall = steps(macroBody("NSIS_HOOK_PREUNINSTALL") ?? "");
  assert.ok(preUninstall.indexOf("!insertmacro OCTOBOARD_CLEAN_UP") < preUninstall.indexOf("!insertmacro OCTOBOARD_REMOVE_FEATURE_ENTRIES"));
});
