import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
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

test("every ConPTY file the installer layout fetches is shipped, and so listed in the hooks", () => {
  const listed = listedFiles().map(({ file }) => file);
  for (const { target } of installerLayout()) assert.ok(listed.includes(target), `${target} is listed`);
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
