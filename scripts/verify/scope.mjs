// Maps the paths a branch changed to the checks that cover them, by the scope rule of `CLAUDE.md`
// "Before delivery", "Formatting and checks": the full set, only files no check reads, or one
// package's checks, and the full set for a shared layer, configuration, dependency versions, a
// change spanning packages, or anything that cannot be judged. Anything not recognised here is the
// last of these.
//
// Every changed path is one of: `unread` (no build, type check or test reads it), a package's own
// path, or `full`. The cross-package relations that make a path `full` are:
//   - what `apps/web` imports from `packages/ui/src`, and the inputs of the preview build it takes
//     from there;
//   - the wire protocol: `apps/daemon/PROTOCOL.md`, `apps/daemon/src/protocol.rs` and
//     `packages/ui/src/protocol.ts`, kept in step by hand and consumed by the UI and the desktop shell;
//   - the contract between `apps/desktop` and `packages/ui`: the Tauri commands and events, the
//     window URL's parameters, the menu message keys and the status item icons. `apps/desktop` itself
//     has no package entry, so every change to it is already `full`;
//   - the modules `packages/ui/scripts/gallery-sweep.mjs` imports from `scripts/verify/`, which its
//     tests (a UI test file) run.

// What `apps/web` imports from `packages/ui/src`: `i18n/matchLanguage.ts` and what it imports.
export const WEB_IMPORTS_FROM_UI = [
  "packages/ui/src/i18n/matchLanguage.ts",
  "packages/ui/src/i18n/languages.ts",
];

// The inputs of the preview build `apps/web` takes from `packages/ui`.
export const WEB_PREVIEW_INPUTS = [
  "packages/ui/vite.preview.config.ts",
  "packages/ui/website-preview.html",
  "packages/ui/src/website-preview/",
];

// Paths that are part of a build configuration or a build script, for the advice to run `pnpm build`.
const BUILD_CONFIG = [
  /^packages\/ui\/vite[^/]*\.config\.ts$/,
  /^packages\/ui\/[^/]*\.html$/,
  /^packages\/ui\/scripts\//,
  /^packages\/ui\/src\/website-preview\//,
  /^apps\/web\/(nuxt\.config\.ts|scripts\/)/,
  /^apps\/desktop\/(scripts\/|package\.json$|src-tauri\/(tauri\.conf\.json|build\.rs)$)/,
  /^apps\/daemon\/build\.rs$/,
  /^scripts\/(?!verify)/,
  /^package\.json$/,
];

// The UI side of the desktop contract: the Tauri bridge, the window URL's parameters, the native
// menu labels (their `menu.*` message keys are `MENU_KEYS_FILE`) and the status item icons.
export const DESKTOP_CONTRACT = [
  "packages/ui/src/platform/index.ts",
  "packages/ui/src/platform/tauri.ts",
  "packages/ui/src/daemon.ts",
  "packages/ui/src/i18n/language.ts",
  "packages/ui/src/lifecycle/useNativeMenuLabels.ts",
  "packages/ui/src/lifecycle/statusItemIcons.ts",
  "packages/ui/src/lifecycle/statusItemIconKeys.ts",
  "packages/ui/src/lifecycle/statusItemMenu.ts",
];
export const MENU_KEYS_FILE = "packages/ui/src/i18n/messages/en.ts";

// The modules of `scripts/verify/` that the gallery sweep also uses.
export const SWEEP_SHARED = ["scripts/verify/lock.mjs", "scripts/verify/stamp.mjs", "scripts/verify/tree.mjs"];

export const WIRE_PROTOCOL = ["apps/daemon/PROTOCOL.md", "apps/daemon/src/protocol.rs", "packages/ui/src/protocol.ts"];

const UNREAD_PREFIXES = ["docs/", ".claude/", ".agents/", ".codex/", ".githooks/"];
const UNREAD_FILES = ["CLAUDE.md", "AGENTS.md", "LICENSE"];

// The packages that have checks of their own, by the paths they own. `verify` is this command's own
// code, which only the gallery sweep depends on (`SWEEP_SHARED`); its checks are its tests and lint.
export const PACKAGE_ROOTS = {
  verify: ["scripts/verify/", "scripts/verify.mjs"],
  ui: ["packages/ui/"],
  web: ["apps/web/"],
  daemon: ["apps/daemon/"],
};

export const belongsTo = (pkg, file) => PACKAGE_ROOTS[pkg].some((root) => file.startsWith(root));

// Manifests and the configuration of type checking, building and testing are what the full-set rule
// calls configuration and dependency versions, whichever package they sit in: a Cargo manifest's
// features, for one, unify across the whole Cargo workspace.
const CONFIGURATION = /(^|\/)(package\.json|Cargo\.toml|build\.rs|tsconfig[^/]*\.json|(vite|vitest|nuxt)[^/]*\.config\.[cm]?[jt]s)$/;

export function classify(file) {
  if (WIRE_PROTOCOL.includes(file) || DESKTOP_CONTRACT.includes(file)) return "full";
  if (WEB_IMPORTS_FROM_UI.includes(file) || SWEEP_SHARED.includes(file)) return "full";
  if (WEB_PREVIEW_INPUTS.some((input) => file.startsWith(input))) return "full";
  if (CONFIGURATION.test(file)) return "full";
  if (UNREAD_FILES.includes(file) || UNREAD_PREFIXES.some((prefix) => file.startsWith(prefix))) return "unread";
  if (file.endsWith(".md")) return "unread";
  return Object.keys(PACKAGE_ROOTS).find((pkg) => belongsTo(pkg, file)) ?? "full";
}

// `files` are the changed paths; `fullFiles` are those among them that a look at the change itself
// showed to touch a cross-package relation (see `MENU_KEYS_FILE`).
export function planScope(files, { fullFiles = [] } = {}) {
  const packages = new Set();
  const full = [];
  for (const file of files) {
    const kind = fullFiles.includes(file) ? "full" : classify(file);
    if (kind === "full") full.push(file);
    else if (kind !== "unread") packages.add(kind);
  }
  const advice = [];
  if (files.some((file) => BUILD_CONFIG.some((pattern) => pattern.test(file)))) {
    advice.push("a build configuration or build script changed: also run `pnpm build`, which this command does not");
  }
  if (full.length > 0) return { mode: "full", reason: `shared or unrecognised: ${preview(full)}`, packages: [], advice };
  if (packages.size > 1) return { mode: "full", reason: `spans packages: ${[...packages].join(", ")}`, packages: [], advice };
  if (packages.size === 1) return { mode: "package", reason: `only ${[...packages][0]}`, packages: [...packages], advice };
  return { mode: "unread", reason: "only files no build, type check or test reads", packages: [], advice };
}

function preview(files) {
  return files.length <= 3 ? files.join(", ") : `${files.slice(0, 3).join(", ")} and ${files.length - 3} more`;
}
