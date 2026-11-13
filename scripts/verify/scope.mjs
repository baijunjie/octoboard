// Maps changed inputs to their consumers: the packages whose checks read them. Every changed file is
// matched against every rule of `RULES` and the results are unioned, so a file that several packages
// read selects each of them without pulling in unrelated ones. A file no rule matches is unknown and
// selects every package; only when none does, a Markdown file is read by no check and selects nothing
// (a rule can therefore make a Markdown file count, as `DAEMON_READS_MARKDOWN` does).
//
// Packages: `verify` is this command's own code, hook scripts and the tests of the scripts directory;
// `daemon` and `native` are the two Rust crates, which the Rust steps always check together (one
// `cargo test --workspace`, one feature set), so selecting either selects both Rust steps.

// What `apps/web` imports from `@octoboard/ui` outside the preview inputs below: the modules its `exports` map
// names (`i18n/matchLanguage.ts`, `i18n/languages.ts`, `providers/agentMarks.ts`) and what they import
// (the provider presentation modules the marks come from, and their types).
export const WEB_IMPORTS_FROM_UI = [
  "packages/ui/src/i18n/matchLanguage.ts",
  "packages/ui/src/i18n/languages.ts",
  "packages/ui/src/providers/agentMarks.ts",
  "packages/ui/src/providers/claude.ts",
  "packages/ui/src/providers/codex.ts",
  "packages/ui/src/providers/grok.ts",
  "packages/ui/src/providers/types.ts",
];

// The inputs of the preview build `apps/web` takes from `packages/ui`.
export const WEB_PREVIEW_INPUTS = [
  "packages/ui/vite.preview.config.ts",
  "packages/ui/website-preview.html",
  "packages/ui/src/website-preview/",
  "packages/ui/src/fixtures/",
];

// The manifest whose `exports` map is how the website resolves `@octoboard/ui`.
export const WEB_RESOLVES_UI = ["packages/ui/package.json"];

// The UI side of the desktop contract: the Tauri bridge, the window URL's parameters, the native
// menu labels (their `menu.*` message keys are `MENU_KEYS_FILE`) and the status item icons.
export const DESKTOP_CONTRACT = [
  "packages/ui/src/platform/index.ts",
  "packages/ui/src/platform/tauri.ts",
  "packages/ui/src/daemon.ts",
  "packages/ui/src/i18n/language.ts",
  "packages/ui/src/lifecycle/useNativeMenuLabels.ts",
  "packages/ui/src/statusItem/statusItemIcons.ts",
  "packages/ui/src/statusItem/statusItemIconKeys.ts",
  "packages/ui/src/statusItem/statusItemMenu.ts",
];
export const MENU_KEYS_FILE = "packages/ui/src/i18n/messages/en.ts";

// The modules of `scripts/verify/` that the UI's gallery sweep also uses, which the UI's tests run: the
// machine-wide heavy-step lock.
export const SWEEP_SHARED = ["scripts/verify/lock.mjs"];

// `protocol.generated.ts` is written by a daemon test from the Rust types (`protocol/bindings.rs`) and re-exported
// by `protocol.ts`, through which the UI reads it. The desktop shell is not a consumer: it does not depend on the
// daemon crate and talks to the daemon only over its WebSocket.
export const WIRE_PROTOCOL = ["apps/daemon/src/protocol/", "packages/ui/src/protocol.ts", "packages/ui/src/protocol.generated.ts"];

// The protocol schema, `contracts/protocol/*.schema.json`, is written by a daemon test from the Rust types
// (`protocol/schema.rs`), which fails when it is stale; `contracts/protocol/released/` is the snapshot of the last
// release that the daemon's compatibility guard (`protocol/compatibility/`) compares it with. Nothing else reads it yet.
export const PROTOCOL_SCHEMA = ["contracts/"];

// Markdown the daemon reads with `include_str!`, which the daemon rule's Markdown exclusion would otherwise leave
// unchecked: the agent instructions compiled into the compatibility sources, and the protocol document, whose table of
// each role's tools a test (`session_tools/catalogue/tests.rs`) compares with the catalogues. `scope.test.mjs` fails
// when a Markdown file a package reads that way is not named.
export const DAEMON_READS_MARKDOWN = ["apps/daemon/PROTOCOL.md", "apps/daemon/src/adapter/claude/compatibility.md", "apps/daemon/src/adapter/codex/compatibility.md"];

// Files another package reads: from disk in a test, or through its bundler's imports. Found by searching for
// `include_str!`, `readFileSync(new URL(...))`, relative imports and relative paths that leave a package root;
// `scope.test.mjs` fails when a new read of that kind is not named here.
// A daemon test (`interface_language.rs`) parses the UI's language list.
export const DAEMON_READS_UI = ["packages/ui/src/i18n/languages.ts"];
// A UI test (`bundleLocalizations.test.ts`) compares the bundle's languages with the shell's Info.plist template.
export const UI_READS_NATIVE = ["apps/desktop/src-tauri/Info.template.plist"];
// Desktop script tests read the native permission catalogs, the template and the macOS overlay's bundle resource
// mappings (`native-localizations.test.mjs`), and the Windows installer hooks with the base and Windows configurations
// whose shipped files the hooks list (`installer-hooks.test.mjs`).
export const DESKTOP_READS_NATIVE = ["apps/desktop/src-tauri/localizations/", "apps/desktop/src-tauri/Info.template.plist", "apps/desktop/src-tauri/tauri.macos.conf.json", "apps/desktop/src-tauri/tauri.conf.json", "apps/desktop/src-tauri/tauri.windows.conf.json", "apps/desktop/src-tauri/windows/installer-hooks.nsh"];
// A daemon test (`platform/build_files.rs`) holds the name the Windows installer hooks give a file they move aside to
// the one the daemon removes, and another (`self_update/tests.rs`) holds the daemon's update key to the updater's.
export const DAEMON_READS_NATIVE = ["apps/desktop/src-tauri/windows/installer-hooks.nsh", "apps/desktop/src-tauri/tauri.conf.json"];
// A website test (`install-script.test.ts`) holds the install script's public key to the updater's.
export const WEB_READS_NATIVE = ["apps/desktop/src-tauri/tauri.conf.json"];
// The same test holds the example systemd unit's port to the configured server port.
export const WEB_READS_DAEMON = ["apps/daemon/dist/systemd/octoboardd.service"];
// A UI test (`appConfig.test.ts`) holds `config/app.json` to the license.
export const UI_READS_ELSEWHERE = ["LICENSE"];
// UI tests (`daemonCodes.test.ts`, `fixtureProviders.test.ts`) read the provider modules' sources, where each provider
// declares its codes and facts.
export const UI_READS_DAEMON = ["apps/daemon/src/providers/"];
// A UI module (`settings/AboutSection.tsx`) inlines the app icon into the UI bundle, and so into the preview of the app
// the website ships.
export const UI_IMPORTS_NATIVE = ["apps/desktop/src-tauri/icons/128x128.png"];
// A verify test (`rerun.test.mjs`) reads the reporter that shares `REPORT_ENV` with `rerun.mjs`.
export const VERIFY_READS_UI = ["packages/ui/src/test/failedFilesReporter.ts"];

export const ALL_PACKAGES = ["verify", "ui", "web", "desktop", "daemon", "native"];
const JS_PACKAGES = ["verify", "ui", "web", "desktop"];
const RUST_PACKAGES = ["daemon", "native"];

// `match` entries are a prefix (ending in "/"), an exact path or a RegExp. A source rule names its own
// directory with a pattern that leaves Markdown out, since documentation is read by no check.
const notMarkdown = (prefix) => new RegExp(`^${prefix.replaceAll("/", "\\/")}(?!.*\\.md$)`);

export const RULES = [
  { match: ["scripts/verify/", "scripts/verify.mjs", "scripts/hooks/", "scripts/lib/", "scripts/format-staged.mjs", "scripts/pre-push.mjs"], consumers: ["verify"], why: "the verification command, the hook scripts, the shared script libraries and their tests (the two top-level hook paths are forwarding stubs for older installed hooks)" },
  { match: [notMarkdown("packages/ui/")], consumers: ["ui"], why: "the UI package" },
  { match: [notMarkdown("apps/web/")], consumers: ["web"], why: "the website" },
  { match: [notMarkdown("apps/daemon/")], consumers: ["daemon"], why: "the daemon crate" },
  { match: [notMarkdown("apps/desktop/src-tauri/")], consumers: ["native"], why: "the desktop shell crate" },
  { match: ["apps/desktop/scripts/"], consumers: ["desktop"], why: "the desktop build and release scripts and their tests" },
  { match: ["apps/desktop/package.json"], consumers: ["desktop", "native"], why: "its scripts are the desktop scripts' commands, `tauri.conf.json` runs them as the shell's build commands, and its version is the application's, which the shell's build script checks" },

  // Cargo features unify across workspace members, so a manifest of either crate changes how both build.
  { match: ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "rustfmt.toml", ".cargo/", /\/Cargo\.toml$/], consumers: RUST_PACKAGES, why: "the Cargo workspace, its feature set and toolchain are shared by both crates" },
  { match: ["pnpm-lock.yaml", "pnpm-workspace.yaml", "patches/", "eslint.config.mjs", "packages/ui/eslint.layers.mjs"], consumers: JS_PACKAGES, why: "dependency versions and the lint rules apply to every JavaScript package; the UI's layering blocks are spread into the root lint configuration" },
  { match: ["package.json"], consumers: ALL_PACKAGES, why: "holds the scripts of every package, including `test:rust`, and the shared dev dependencies" },
  { match: ["scripts/lib/git.mjs"], consumers: ["verify", "ui", "desktop"], why: "imported by `scripts/verify.mjs`, `scripts/hooks/*.mjs`, `scripts/verify/tree.mjs` and `scripts/lib/env.mjs` (so by the UI's `vite.config.ts` too), and by the desktop release publishing script" },
  { match: ["scripts/lib/env.mjs", "scripts/lib/env.d.mts"], consumers: ["verify", "ui", "desktop"], why: "imported by `scripts/verify/lock.mjs`, `packages/ui/vite.config.ts` (its declaration types that import) and the desktop release scripts" },
  { match: ["scripts/cargo.mjs"], consumers: ["verify", ...RUST_PACKAGES], why: "the command the Rust check steps run (and the Linux CI job), linted with the scripts" },
  { match: ["scripts/lib/windows-toolchain.mjs"], consumers: ["verify", "desktop", ...RUST_PACKAGES], why: "imported by `scripts/cargo.mjs`, which every Rust check step runs, and by the desktop release script, which checks the Windows cross-check's toolchain before preparing a version" },
  { match: ["scripts/verify/environment.mjs"], consumers: ["verify", "desktop"], why: "verification entry points and foreign Git fixtures, including desktop release tests, refuse inherited repository locations" },
  { match: ["scripts/sync-identity.mjs", "scripts/sync-identity.test.mjs"], consumers: ["verify"], why: "generates and checks the LICENSE and Cargo author fields from config/app.json" },
  { match: ["scripts/fetch-conpty.mjs", "scripts/fetch-conpty.test.mjs"], consumers: ["verify", "desktop"], why: "fetches the bundled ConPTY the Windows daemon tests and CI run on, tested with the scripts; the desktop Windows release scripts and the installer hooks test import it (`windows-release.mjs` and `installer-hooks.test.mjs` take the installer layout from it), so their tests run too" },
  { match: [".githooks/"], consumers: ["verify"], why: "the hook entry point is exercised by the pre-push tests; nothing else reads it" },
  { match: [".gitignore"], consumers: ["verify"], why: "decides which files the working-tree snapshot (`tree.mjs`) counts; no other check reads it" },
  { match: ["config/"], consumers: ALL_PACKAGES, builds: ["ui"], why: "`config/app.json` is the product identity every package reads, and `vite.config.ts` bakes it into the UI build" },

  // Nothing local reads these. Listed so that they do not fall through to "unknown".
  { match: ["docs/", ".claude/", ".agents/", ".codex/", "assets/branding/", ".github/", "CLAUDE.md", "AGENTS.md"], consumers: [], why: "documentation, agent files, brand assets and CI definitions are read by no local check" },

  // Cross-package reads. Each also selects `verify`, whose tests check that the paths named here exist.
  { match: [...WEB_IMPORTS_FROM_UI, ...WEB_PREVIEW_INPUTS, ...WEB_RESOLVES_UI], consumers: ["verify", "ui", "web"], why: "the website imports these modules, builds its preview from them, or resolves the package through its `exports` map" },
  { match: WIRE_PROTOCOL, consumers: ["verify", "ui", "daemon"], why: "the wire protocol is kept in step across the daemon and the UI: the UI reads the generated types, which the daemon writes from its own, and a UI test (`daemonCodes.test.ts`) reads the daemon's code registry from `protocol/codes.rs`" },
  { match: PROTOCOL_SCHEMA, consumers: ["verify", "daemon"], why: "the protocol schema is written by a daemon test from the daemon's types, and the same test fails when it is stale; the compatibility guard reads it and its release snapshot" },
  { match: DESKTOP_CONTRACT, consumers: ["verify", "ui", "native"], why: "the UI side of the contract with the desktop shell" },
  { match: SWEEP_SHARED, consumers: ["verify", "ui"], why: "the UI's gallery sweep, which its tests run, imports it" },
  { match: DAEMON_READS_UI, consumers: ["verify", "ui", "daemon"], why: "a daemon test parses it" },
  { match: UI_READS_DAEMON, consumers: ["verify", "ui", "daemon"], why: "a UI test reads them" },
  { match: UI_READS_NATIVE, consumers: ["verify", "ui", "native"], why: "a UI test reads it" },
  { match: DESKTOP_READS_NATIVE, consumers: ["verify", "desktop", "native"], why: "desktop script tests read the native permission catalogs, template, the macOS bundle resource mappings, and the Windows installer hooks with the configurations whose files they list" },
  { match: DAEMON_READS_NATIVE, consumers: ["verify", "daemon", "native"], why: "a daemon test reads it" },
  { match: WEB_READS_NATIVE, consumers: ["verify", "web", "native"], why: "a website test holds the install script's key to the updater's" },
  { match: WEB_READS_DAEMON, consumers: ["verify", "web", "daemon"], why: "a website test holds the example unit's port to the install script's" },
  { match: UI_READS_ELSEWHERE, consumers: ["verify", "ui"], why: "a UI test reads it" },
  { match: VERIFY_READS_UI, consumers: ["verify", "ui"], why: "a verify test reads it" },
  { match: UI_IMPORTS_NATIVE, consumers: ["verify", "ui", "web"], builds: ["ui"], why: "the UI bundle inlines it, and the website's preview of the app is built from that bundle" },
  { match: DAEMON_READS_MARKDOWN, consumers: ["verify", "daemon"], why: "read by the daemon with `include_str!`, compiled in or as a test's input" },

  // The UI build runs only for changes to what it reads besides `src/`. The gallery and benchmark
  // scripts and pages are not among them, and the website's preview build has no step of its own.
  { match: ["packages/ui/vite.config.ts", "packages/ui/index.html", "packages/ui/typography.mjs", "packages/ui/scripts/generate-licenses.mjs", "packages/ui/package.json", "packages/ui/tsconfig.json", "packages/ui/tsconfig.app.json", "Cargo.lock", "pnpm-lock.yaml", "scripts/lib/env.mjs", "scripts/lib/git.mjs"], consumers: [], builds: ["ui"], why: "the inputs of `pnpm build` of the UI: the bundler configuration, the page, and the lock files and manifests the license generator reads" },
];

export const matches = (matcher, file) => matcher instanceof RegExp ? matcher.test(file) : matcher.endsWith("/") ? file.startsWith(matcher) : file === matcher;

export const matchingRules = (file) => RULES.filter((rule) => rule.match.some((matcher) => matches(matcher, file)));

export function planScope(files, { fullFiles = [] } = {}) {
  const packages = new Set();
  const builds = new Set();
  for (const file of files) {
    const hits = matchingRules(file);
    for (const rule of hits) {
      rule.consumers.forEach((name) => packages.add(name));
      rule.builds?.forEach((name) => builds.add(name));
    }
    if (hits.length === 0 && !file.endsWith(".md")) ALL_PACKAGES.forEach((name) => packages.add(name));
    // The message catalog's `menu.*` keys are read by the desktop shell's menu, but only a change to
    // one of those lines counts (the caller decides, from the diff).
    if (fullFiles.includes(file)) ["verify", "ui", "native"].forEach((name) => packages.add(name));
  }
  const selected = ALL_PACKAGES.filter((name) => packages.has(name));
  return {
    mode: selected.length === 0 && builds.size === 0 ? "unread" : "package",
    reason: selected.length === 0 && builds.size === 0 ? "no checked input changed" : `affected: ${selected.join(", ")}`,
    packages: selected, builds: [...builds],
  };
}
