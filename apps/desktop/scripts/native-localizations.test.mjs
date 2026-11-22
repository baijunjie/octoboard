import { test } from "node:test";
import assert from "node:assert/strict";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";

import { appleLocalization, generateNativeLocalizations } from "./native-localizations.mjs";

const read = (file) => readFileSync(new URL(file, import.meta.url), "utf8");
const catalogs = Object.fromEntries(readdirSync(new URL("../src-tauri/localizations/", import.meta.url)).filter((file) => file.endsWith(".json")).sort().map((file) => [file.slice(0, -5), JSON.parse(read(`../src-tauri/localizations/${file}`))]));
const basePlist = read("../src-tauri/Info.template.plist");
// The macOS permission prompts of a catalog; the rest is the shell's other native text (`src/localizations.rs`).
const permissions = (catalog) => Object.fromEntries(Object.entries(catalog).filter(([key]) => /^NS\w+UsageDescription$/.test(key)));

test("provides a native permission catalog for every offered bundle language", () => {
  const array = /<key>CFBundleLocalizations<\/key>\s*<array>([\s\S]*?)<\/array>/.exec(basePlist)?.[1] ?? "";
  const languages = [...array.matchAll(/<string>([^<]+)<\/string>/g)].map((match) => match[1]);
  assert.ok(languages.length > 0);
  assert.deepEqual(Object.keys(catalogs).map(appleLocalization).sort(), languages.sort());
});

test("generates all native purposes with the configured identity and valid bundle resources", () => {
  const outputDir = mkdtempSync(path.join(tmpdir(), "native-localizations-"));
  try {
    const appName = 'Test & <App> "Name"';
    generateNativeLocalizations({ appName, catalogs, outputDir, basePlist });
    assert.equal(Object.keys(permissions(catalogs.en)).length, 15);
    const config = JSON.parse(read("../src-tauri/tauri.macos.conf.json"));
    assert.equal(config.bundle.macOS.infoPlist, "Info.plist");
    for (const language of Object.keys(catalogs)) {
      const resource = `${appleLocalization(language)}.lproj/InfoPlist.strings`;
      assert.equal(config.bundle.resources[`generated-localizations/${resource}`], resource);
      const file = path.join(outputDir, resource);
      const strings = readFileSync(file, "utf8");
      assert.ok(!strings.includes("{appName}"));
      assert.ok(!strings.includes("Octoboard"));
      assert.equal(strings.split("\n").filter(Boolean).length, 15);
      if (process.platform === "darwin") {
        execFileSync("plutil", ["-lint", file]);
        const values = JSON.parse(execFileSync("plutil", ["-convert", "json", "-o", "-", file], { encoding: "utf8" }));
        for (const [key, template] of Object.entries(permissions(catalogs[language]))) assert.equal(values[key], template.replaceAll("{appName}", appName));
      }
    }
    const plist = path.join(outputDir, "Info.plist");
    assert.ok(readFileSync(plist, "utf8").includes("Test &amp; &lt;App&gt; &quot;Name&quot;"));
    if (process.platform === "darwin") {
      execFileSync("plutil", ["-lint", plist]);
      const values = JSON.parse(execFileSync("plutil", ["-convert", "json", "-o", "-", plist], { encoding: "utf8" }));
      assert.equal(values.CFBundleDevelopmentRegion, "en");
      assert.ok(values.CFBundleLocalizations.includes("zh-Hans"));
      for (const [key, template] of Object.entries(permissions(catalogs.en))) assert.equal(values[key], template.replaceAll("{appName}", appName));
    }
  } finally {
    rmSync(outputDir, { recursive: true, force: true });
  }
});

test("the CLI writes the same permission plist for Tauri dev embedding and bundled builds", () => {
  const root = mkdtempSync(path.join(tmpdir(), "native-localizations-cli-"));
  try {
    const desktop = path.join(root, "apps/desktop");
    const script = path.join(desktop, "scripts/native-localizations.mjs");
    mkdirSync(path.dirname(script), { recursive: true });
    mkdirSync(path.join(desktop, "src-tauri/localizations"), { recursive: true });
    mkdirSync(path.join(root, "config"));
    cpSync(new URL("./native-localizations.mjs", import.meta.url), script);
    writeFileSync(path.join(root, "config/app.json"), JSON.stringify({ name: "Test application" }));
    writeFileSync(path.join(desktop, "src-tauri/Info.template.plist"), basePlist);
    for (const [language, catalog] of Object.entries(catalogs)) {
      writeFileSync(path.join(desktop, `src-tauri/localizations/${language}.json`), JSON.stringify(catalog));
    }
    execFileSync(process.execPath, [script]);
    for (const language of Object.keys(catalogs)) {
      const generated = readFileSync(path.join(desktop, `src-tauri/generated-localizations/${appleLocalization(language)}.lproj/InfoPlist.strings`), "utf8");
      assert.equal(generated.split("\n").filter(Boolean).length, 15);
      assert.ok(!generated.includes("{appName}"));
    }
    const developmentPlist = path.join(desktop, "src-tauri/Info.plist");
    const config = JSON.parse(read("../src-tauri/tauri.macos.conf.json"));
    assert.equal(path.join(desktop, "src-tauri", config.bundle.macOS.infoPlist), developmentPlist);
    const content = readFileSync(developmentPlist, "utf8");
    for (const [key, template] of Object.entries(permissions(catalogs.en))) {
      assert.ok(content.includes(`<key>${key}</key>`));
      assert.ok(content.includes(template.replaceAll("{appName}", "Test application")));
    }
    assert.ok(!content.includes("{appName}"));
    if (process.platform === "darwin") execFileSync("plutil", ["-lint", developmentPlist]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("an Apple localization identifier is Octoboard's tag, except for Chinese, which Apple names by script", () => {
  assert.equal(appleLocalization("zh-CN"), "zh-Hans");
  assert.equal(appleLocalization("zh-TW"), "zh-Hant");
  for (const language of ["en", "de", "ja", "ar", "pt"]) assert.equal(appleLocalization(language), language);
  for (const name of ["constructor", "toString", "__proto__"]) assert.equal(appleLocalization(name), name);
});

test("the bundle's localization directories are named by Apple's identifiers, never by Octoboard's Chinese tags", () => {
  assert.ok(Object.keys(catalogs).includes("zh-CN") && Object.keys(catalogs).includes("zh-TW"));
  const outputDir = mkdtempSync(path.join(tmpdir(), "native-localizations-names-"));
  try {
    generateNativeLocalizations({ appName: "Test", catalogs, outputDir, basePlist });
    const directories = readdirSync(outputDir).filter((name) => name.endsWith(".lproj"));
    assert.ok(directories.includes("zh-Hans.lproj") && directories.includes("zh-Hant.lproj"));
    assert.ok(!directories.includes("zh-CN.lproj") && !directories.includes("zh-TW.lproj"));
    assert.equal(directories.length, Object.keys(catalogs).length);
    // The Simplified catalog's own text is in the Simplified directory and the Traditional one's in the Traditional.
    const strings = (directory) => readFileSync(path.join(outputDir, directory, "InfoPlist.strings"), "utf8");
    assert.ok(strings("zh-Hans.lproj").includes(JSON.stringify(catalogs["zh-CN"].NSCameraUsageDescription.replaceAll("{appName}", "Test"))));
    assert.ok(strings("zh-Hant.lproj").includes(JSON.stringify(catalogs["zh-TW"].NSCameraUsageDescription.replaceAll("{appName}", "Test"))));
  } finally {
    rmSync(outputDir, { recursive: true, force: true });
  }
});

test("every catalog the shell's build script watches exists", () => {
  const watched = [...read("../src-tauri/build.rs").matchAll(/"(localizations\/[^"]+\.json)"/g)].map((match) => match[1]);
  assert.ok(watched.length > 0);
  for (const file of watched) assert.ok(existsSync(new URL(`../src-tauri/${file}`, import.meta.url)), file);
});

test("refuses missing translations and unresolved template parameters", () => {
  assert.throws(() => generateNativeLocalizations({ appName: "Test", catalogs: { ...catalogs, "zh-CN": {} }, outputDir: "unused", basePlist }), /keys differ/);
  assert.throws(() => generateNativeLocalizations({ appName: "Test", catalogs: { en: { NSCameraUsageDescription: "{appName} {unknown}" } }, outputDir: "unused", basePlist }), /template/);
});

test("the shell reads every catalog, and only the permission prompts become bundle resources", () => {
  const sources = [...read("../src-tauri/src/localizations.rs").matchAll(/\("([^"]+)", include_str!\("\.\.\/localizations\/([^"]+)\.json"\)\)/g)];
  assert.deepEqual(sources.map((match) => match[1]), sources.map((match) => match[2]), "each tag reads its own file");
  assert.deepEqual(sources.map((match) => match[1]).sort(), Object.keys(catalogs).sort());
  const outputDir = mkdtempSync(path.join(tmpdir(), "native-localizations-other-"));
  try {
    generateNativeLocalizations({ appName: "Test", catalogs, outputDir, basePlist });
    assert.doesNotMatch(readFileSync(path.join(outputDir, "en.lproj", "InfoPlist.strings"), "utf8"), /uninstallCleanUp/);
  } finally {
    rmSync(outputDir, { recursive: true, force: true });
  }
});

// A message's placeholders, sorted.
const placeholders = (text) => [...text.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();

test("every catalog has the shell's other native text in full, with English's placeholders", () => {
  const other = (catalog) => Object.keys(catalog).filter((key) => !/^NS\w+UsageDescription$/.test(key)).sort();
  assert.ok(other(catalogs.en).length > 0);
  for (const [language, catalog] of Object.entries(catalogs)) {
    assert.deepEqual(other(catalog), other(catalogs.en), language);
    for (const key of other(catalog)) {
      assert.deepEqual(placeholders(catalog[key]), placeholders(catalogs.en[key]), `${key} in ${language}`);
      assert.ok(catalog[key].length > 0, `${key} in ${language}`);
    }
  }
});

test("in Arabic, each left-to-right value of the shell's other native text is set off by left-to-right marks", () => {
  // Names, hosts, paths and reasons are left-to-right text inside a right-to-left sentence; a Win32 message box shows
  // them in order only between marks. The parts of a list, the seconds and the count are already Arabic or digits.
  const own = new Set(["parts", "seconds", "count"]);
  for (const [key, text] of Object.entries(catalogs.ar).filter(([key]) => key.startsWith("uninstallCleanUp."))) {
    for (const match of text.matchAll(/\{(\w+)\}/g)) {
      if (own.has(match[1])) continue;
      assert.equal(text[match.index - 1], "‎", `${key}: before {${match[1]}}`);
      assert.equal(text[match.index + match[0].length], "‎", `${key}: after {${match[1]}}`);
    }
  }
});

test("in French, the shell's other native text puts a narrow no-break space before a colon or semicolon", () => {
  for (const [key, text] of Object.entries(catalogs.fr).filter(([key]) => key.startsWith("uninstallCleanUp."))) {
    assert.doesNotMatch(text, / [:;]|[^\u202f][:;]/, key);
  }
});
