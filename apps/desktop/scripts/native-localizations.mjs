import { readFileSync, readdirSync, realpathSync, mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const desktopDir = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const json = (file) => JSON.parse(readFileSync(file, "utf8"));
const xml = (value) => value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;").replaceAll("'", "&apos;");

/** The catalogs are named by Octoboard's language tags; a bundle's `.lproj` directory is named by Apple's localization
 * identifier, which Xcode and the App Store recognise, and which for Chinese is the script tag rather than the region
 * tag Octoboard uses. This is the one place generation maps the two; everywhere else a language is its tag in
 * `packages/ui/src/i18n/languages.ts`. `src-tauri/Info.template.plist` and `src-tauri/tauri.macos.conf.json` name the
 * Apple identifiers too, held to this map by `native-localizations.test.mjs` and
 * `packages/ui/src/i18n/bundleLocalizations.test.ts`. */
const APPLE_LOCALIZATIONS = new Map([["zh-CN", "zh-Hans"], ["zh-TW", "zh-Hant"]]);

/** The Apple localization identifier of one of Octoboard's language tags. */
export const appleLocalization = (language) => APPLE_LOCALIZATIONS.get(language) ?? language;

/** The macOS permission prompts of a catalog, sorted: its `NS…UsageDescription` keys. The catalogs hold the shell's
 * other native text as well (`src-tauri/src/localizations.rs`), which is no bundle resource. */
const permissionKeys = (catalog) => Object.keys(catalog).filter((key) => /^NS\w+UsageDescription$/.test(key)).sort();

/** Generates macOS bundle resources before Tauri builds or launches. Permission prompts use the bundle's language,
 * independently of the language selected inside the application. */
export function generateNativeLocalizations({ appName, catalogs, outputDir, basePlist, infoPlistPath = path.join(outputDir, "Info.plist") }) {
  const keys = permissionKeys(catalogs.en);
  if (keys.length === 0) throw new Error("Invalid native permission keys");
  const render = (text) => {
    if (typeof text !== "string" || !text.includes("{appName}") || /\{(?!appName\})/.test(text)) throw new Error("Invalid native permission template");
    return text.replaceAll("{appName}", () => appName);
  };
  for (const [language, catalog] of Object.entries(catalogs)) {
    if (JSON.stringify(permissionKeys(catalog)) !== JSON.stringify(keys)) throw new Error(`Native permission keys differ for ${language}`);
    for (const key of keys) render(catalog[key]);
  }
  for (const [language, catalog] of Object.entries(catalogs)) {
    const directory = path.join(outputDir, `${appleLocalization(language)}.lproj`);
    mkdirSync(directory, { recursive: true });
    const strings = keys.map((key) => `${JSON.stringify(key)} = ${JSON.stringify(render(catalog[key]))};`).join("\n");
    writeFileSync(path.join(directory, "InfoPlist.strings"), `${strings}\n`);
  }
  // The base values also cover system languages without a native catalog.
  const values = keys.map((key) => `  <key>${key}</key>\n  <string>${xml(render(catalogs.en[key]))}</string>`).join("\n");
  writeFileSync(infoPlistPath, basePlist.replace("</dict>", `${values}\n</dict>`));
}

if (process.argv[1] && import.meta.url === pathToFileURL(realpathSync(process.argv[1])).href) {
  const { name } = json(path.join(desktopDir, "../../config/app.json"));
  const catalogDir = path.join(desktopDir, "src-tauri/localizations");
  const catalogs = Object.fromEntries(readdirSync(catalogDir).filter((file) => file.endsWith(".json")).sort().map((file) => [file.slice(0, -5), json(path.join(catalogDir, file))]));
  generateNativeLocalizations({ appName: name, catalogs, outputDir: path.join(desktopDir, "src-tauri/generated-localizations"), basePlist: readFileSync(path.join(desktopDir, "src-tauri/Info.template.plist"), "utf8"), infoPlistPath: path.join(desktopDir, "src-tauri/Info.plist") });
}
