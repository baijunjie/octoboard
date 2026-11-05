import { test } from "node:test";
import assert from "node:assert/strict";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";

import { generateNativeLocalizations } from "./native-localizations.mjs";

const read = (file) => readFileSync(new URL(file, import.meta.url), "utf8");
const catalogs = Object.fromEntries(readdirSync(new URL("../src-tauri/localizations/", import.meta.url)).filter((file) => file.endsWith(".json")).sort().map((file) => [file.slice(0, -5), JSON.parse(read(`../src-tauri/localizations/${file}`))]));
const basePlist = read("../src-tauri/Info.template.plist");

test("provides a native permission catalog for every offered bundle language", () => {
  const array = /<key>CFBundleLocalizations<\/key>\s*<array>([\s\S]*?)<\/array>/.exec(basePlist)?.[1] ?? "";
  const languages = [...array.matchAll(/<string>([^<]+)<\/string>/g)].map((match) => match[1]);
  assert.ok(languages.length > 0);
  assert.deepEqual(Object.keys(catalogs).sort(), languages.sort());
});

test("generates all native purposes with the configured identity and valid bundle resources", () => {
  const outputDir = mkdtempSync(path.join(tmpdir(), "native-localizations-"));
  try {
    const appName = 'Test & <App> "Name"';
    generateNativeLocalizations({ appName, catalogs, outputDir, basePlist });
    assert.equal(Object.keys(catalogs.en).length, 15);
    const config = JSON.parse(read("../src-tauri/tauri.macos.conf.json"));
    assert.equal(config.bundle.macOS.infoPlist, "Info.plist");
    for (const language of Object.keys(catalogs)) {
      const resource = `${language}.lproj/InfoPlist.strings`;
      assert.equal(config.bundle.resources[`generated-localizations/${resource}`], resource);
      const file = path.join(outputDir, resource);
      const strings = readFileSync(file, "utf8");
      assert.ok(!strings.includes("{appName}"));
      assert.ok(!strings.includes("Octoboard"));
      assert.equal(strings.split("\n").filter(Boolean).length, 15);
      if (process.platform === "darwin") {
        execFileSync("plutil", ["-lint", file]);
        const values = JSON.parse(execFileSync("plutil", ["-convert", "json", "-o", "-", file], { encoding: "utf8" }));
        for (const [key, template] of Object.entries(catalogs[language])) assert.equal(values[key], template.replaceAll("{appName}", appName));
      }
    }
    const plist = path.join(outputDir, "Info.plist");
    assert.ok(readFileSync(plist, "utf8").includes("Test &amp; &lt;App&gt; &quot;Name&quot;"));
    if (process.platform === "darwin") {
      execFileSync("plutil", ["-lint", plist]);
      const values = JSON.parse(execFileSync("plutil", ["-convert", "json", "-o", "-", plist], { encoding: "utf8" }));
      assert.equal(values.CFBundleDevelopmentRegion, "en");
      assert.ok(values.CFBundleLocalizations.includes("zh-Hans"));
      for (const [key, template] of Object.entries(catalogs.en)) assert.equal(values[key], template.replaceAll("{appName}", appName));
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
      const generated = readFileSync(path.join(desktop, `src-tauri/generated-localizations/${language}.lproj/InfoPlist.strings`), "utf8");
      assert.equal(generated.split("\n").filter(Boolean).length, 15);
      assert.ok(!generated.includes("{appName}"));
    }
    const developmentPlist = path.join(desktop, "src-tauri/Info.plist");
    const config = JSON.parse(read("../src-tauri/tauri.macos.conf.json"));
    assert.equal(path.join(desktop, "src-tauri", config.bundle.macOS.infoPlist), developmentPlist);
    const content = readFileSync(developmentPlist, "utf8");
    for (const [key, template] of Object.entries(catalogs.en)) {
      assert.ok(content.includes(`<key>${key}</key>`));
      assert.ok(content.includes(template.replaceAll("{appName}", "Test application")));
    }
    assert.ok(!content.includes("{appName}"));
    if (process.platform === "darwin") execFileSync("plutil", ["-lint", developmentPlist]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("refuses missing translations and unresolved template parameters", () => {
  assert.throws(() => generateNativeLocalizations({ appName: "Test", catalogs: { ...catalogs, "zh-Hans": {} }, outputDir: "unused", basePlist }), /keys differ/);
  assert.throws(() => generateNativeLocalizations({ appName: "Test", catalogs: { en: { NSCameraUsageDescription: "{appName} {unknown}" } }, outputDir: "unused", basePlist }), /template/);
});
