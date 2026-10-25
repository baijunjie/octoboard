// Writes `src/about/licenses.generated.json`: the licenses of the third-party code the application ships,
// which Settings' About section lists (`src/about/`). Two sources:
//
// - the npm packages in the production dependency closure of this package (`dependencies`, and what each of
//   those depends on in turn), read from `node_modules`. A package whose output the bundle takes in, such as a
//   stylesheet or a Tailwind plugin the CSS names, is therefore a `dependencies` entry, never a dev one (a test
//   holds the CSS to that). `@types/*` packages are left out: they hold declarations, which ship nothing;
// - the Cargo crates the desktop shell and the daemon are built from (their normal dependencies, for the two
//   macOS targets the application is built for), read through `cargo metadata`, which may fetch the registry
//   index and the crates' sources a fresh checkout does not have yet. Procedural macros run in the compiler and
//   are not part of the binaries, so a crate that is one, and whatever only it needs, is left out.
//
// Each entry carries its SPDX license expression and the license files its package ships; a text shared by
// several packages is stored once. Run by `pnpm build` ahead of the bundle, and on its own as
// `pnpm licenses:generate`.

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, realpathSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const PACKAGE_DIR = path.resolve(here, "..");
const REPOSITORY_DIR = path.resolve(PACKAGE_DIR, "../..");
export const OUTPUT_FILE = path.join(PACKAGE_DIR, "src/about/licenses.generated.json");

/** The targets the application is built for; a crate only another platform needs is not shipped. */
const CARGO_TARGETS = ["aarch64-apple-darwin", "x86_64-apple-darwin"];

const LICENSE_FILE = /^(licen[cs]e|copying|unlicense|notice)([-._ ].*)?$/i;
const NOT_TEXT = /\.(js|mjs|cjs|ts|json|rs|png|jpe?g|svg|gif|wasm|node|map)$/i;

/** The text of a license file, with the line endings and the blank edges every copy has made uniform. */
function readText(file) {
  return readFileSync(file, "utf8").replace(/\r\n?/g, "\n").trim();
}

/** The license files a package ships at its top level, in file name order, plus `extra` (a file its manifest
 * names), as the texts they hold. */
export function readLicenseTexts(directory, extra) {
  const names = new Set(
    readdirSync(directory).filter((name) => LICENSE_FILE.test(name) && !NOT_TEXT.test(name) && statSync(path.join(directory, name)).isFile()),
  );
  if (extra && existsSync(path.join(directory, extra)) && statSync(path.join(directory, extra)).isFile()) names.add(extra);
  return [...names].sort().map((name) => readText(path.join(directory, name))).filter((text) => text !== "");
}

/** The SPDX expression a package.json declares: `license` as a string or `{ type }`, or the old `licenses` array
 * of them, offered as alternatives. */
export function npmLicense(manifest) {
  const one = (value) => (typeof value === "string" ? value : value?.type);
  const single = one(manifest.license);
  if (single) return single;
  if (Array.isArray(manifest.licenses)) {
    const all = manifest.licenses.map(one).filter(Boolean);
    if (all.length > 0) return all.join(" OR ");
  }
  return undefined;
}

/** The directory of the package `name` as `from` sees it, by Node's own search: the `node_modules` beside or
 * above `from` (pnpm keeps a package's dependencies beside it, in its store directory's `node_modules`). */
function resolvePackage(name, from) {
  for (let directory = from; ; directory = path.dirname(directory)) {
    const base = path.basename(directory) === "node_modules" ? directory : path.join(directory, "node_modules");
    const candidate = path.join(base, name);
    if (existsSync(path.join(candidate, "package.json"))) return realpathSync(candidate);
    if (path.dirname(directory) === directory) return undefined;
  }
}

/** Whether `name` is a DefinitelyTyped package, whose declarations the build reads and never ships. */
export function isTypesPackage(name) {
  return typeof name === "string" && name.startsWith("@types/");
}

/** Every npm package `root`'s `dependencies` need to run, transitively: their `dependencies`, and the
 * `optionalDependencies` and `peerDependencies` that are installed. A required dependency that is not
 * installed is an error, since the list would otherwise silently miss a package that ships. */
export function collectNpmPackages(root) {
  const rootManifest = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8"));
  const found = new Map();
  const queue = Object.keys(rootManifest.dependencies ?? {}).map((name) => ({ name, from: root, required: true }));
  const seen = new Set();
  while (queue.length > 0) {
    const { name, from, required } = queue.shift();
    const directory = resolvePackage(name, from);
    if (!directory) {
      if (required) throw new Error(`${name} is a dependency of ${from} but is not installed; run pnpm install`);
      continue;
    }
    if (seen.has(directory)) continue;
    seen.add(directory);
    const manifest = JSON.parse(readFileSync(path.join(directory, "package.json"), "utf8"));
    if (isTypesPackage(manifest.name)) continue;
    if (!manifest.private) {
      const license = npmLicense(manifest);
      const named = /^SEE LICENSE IN (.+)$/i.exec(license ?? "")?.[1];
      found.set(`${manifest.name}@${manifest.version}`, {
        name: manifest.name,
        version: manifest.version,
        ecosystem: "npm",
        license: named || !license ? "See license file" : license,
        texts: readLicenseTexts(directory, named),
      });
    }
    for (const [group, isRequired] of [["dependencies", true], ["optionalDependencies", false], ["peerDependencies", false]]) {
      for (const dependency of Object.keys(manifest[group] ?? {})) queue.push({ name: dependency, from: directory, required: isRequired });
    }
  }
  return [...found.values()];
}

/** Whether `crate` is a procedural macro, which runs in the compiler and is not linked into what ships. Its
 * `proc-macro` target may come with others (an example), so one such target is enough. */
function isProcMacro(crate) {
  return (crate.targets ?? []).some((target) => target.kind.includes("proc-macro"));
}

/** The crates the workspace's members are built from on `target`, as `cargo metadata` describes them: what
 * their normal (not build or dev) dependencies reach, the members themselves left out. A procedural macro is
 * not reached, so what only it depends on is not either. */
export function cratesFromMetadata(metadata) {
  const members = new Set(metadata.workspace_members);
  const byId = new Map(metadata.packages.map((crate) => [crate.id, crate]));
  const nodes = new Map(metadata.resolve.nodes.map((node) => [node.id, node]));
  const reached = new Set();
  const queue = [...members];
  while (queue.length > 0) {
    const id = queue.pop();
    for (const dependency of nodes.get(id)?.deps ?? []) {
      if (!dependency.dep_kinds.some((kind) => kind.kind === null) || reached.has(dependency.pkg)) continue;
      if (isProcMacro(byId.get(dependency.pkg))) continue;
      reached.add(dependency.pkg);
      queue.push(dependency.pkg);
    }
  }
  return [...reached].filter((id) => !members.has(id)).map((id) => byId.get(id));
}

/** Every Cargo crate that ships in the application, for all of `CARGO_TARGETS`. */
export function collectCargoCrates(repository) {
  const crates = new Map();
  for (const target of CARGO_TARGETS) {
    let output;
    try {
      output = execFileSync("cargo", ["metadata", "--format-version", "1", "--locked", "--filter-platform", target], {
        cwd: repository,
        encoding: "utf8",
        maxBuffer: 256 * 1024 * 1024,
        stdio: ["ignore", "pipe", "pipe"],
      });
    } catch (error) {
      if (error.code === "ENOENT") throw new Error("cargo was not found on PATH; the Rust crates' licenses are read through it, so install a Rust toolchain", { cause: error });
      throw new Error(`cargo metadata failed for ${target}: ${error.stderr || error.message}`, { cause: error });
    }
    for (const crate of cratesFromMetadata(JSON.parse(output))) {
      if (crates.has(crate.id)) continue;
      const directory = path.dirname(crate.manifest_path);
      crates.set(crate.id, {
        name: crate.name,
        version: crate.version,
        ecosystem: "cargo",
        license: crate.license ?? "See license file",
        texts: readLicenseTexts(directory, crate.license_file),
      });
    }
  }
  return [...crates.values()];
}

/** The document the UI loads: the packages by name and version, each pointing at its license texts by
 * position in `texts`, where a text shared by several packages is held once. */
export function buildDocument(packages) {
  const texts = [];
  const indexOf = new Map();
  const entries = packages
    .map(({ texts: own, ...rest }) => ({
      ...rest,
      texts: own.map((text) => {
        if (!indexOf.has(text)) indexOf.set(text, texts.push(text) - 1);
        return indexOf.get(text);
      }),
    }))
    .sort((a, b) => a.name.localeCompare(b.name, "en") || a.version.localeCompare(b.version, "en", { numeric: true }) || a.ecosystem.localeCompare(b.ecosystem));
  return { texts, packages: entries };
}

/** The packages with no license text of their own, which the build reports rather than hides. */
export function withoutText(document) {
  return document.packages.filter((entry) => entry.texts.length === 0);
}

function main() {
  const packages = [...collectNpmPackages(PACKAGE_DIR), ...collectCargoCrates(REPOSITORY_DIR)];
  const document = buildDocument(packages);
  mkdirSync(path.dirname(OUTPUT_FILE), { recursive: true });
  writeFileSync(OUTPUT_FILE, JSON.stringify(document));
  const bare = withoutText(document);
  console.log(`licenses: ${document.packages.length} packages, ${document.texts.length} distinct texts, ${statSync(OUTPUT_FILE).size} bytes -> ${path.relative(REPOSITORY_DIR, OUTPUT_FILE)}`);
  if (bare.length > 0) console.warn(`licenses: no license file shipped by ${bare.map((entry) => `${entry.name}@${entry.version}`).join(", ")}`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
