// Maps changed inputs to their consumers: the packages whose checks read them. Every changed file is
// matched against every rule of `RULES` and the results are unioned, so a file that several packages
// read selects each of them without pulling in unrelated ones. A file no rule matches is unknown and
// selects every package; only when none does, a Markdown file is read by no check and selects nothing
// (a rule can therefore make a Markdown file count, as the wire protocol document does).
//
// Packages: `verify` is this command's own code, hook scripts and the tests of the scripts directory;
// `daemon` and `native` are the two Rust crates, which the Rust steps always check together (one
// `cargo test --workspace`, one feature set), so selecting either selects both Rust steps.

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

// The modules of `scripts/verify/` that the UI's gallery sweep also uses, which the UI's tests run: the
// machine-wide heavy-step lock.
export const SWEEP_SHARED = ["scripts/verify/lock.mjs"];

export const WIRE_PROTOCOL = ["apps/daemon/PROTOCOL.md", "apps/daemon/src/protocol.rs", "packages/ui/src/protocol.ts"];

// Files a test of another package reads from disk. Found by searching for `include_str!`,
// `readFileSync(new URL(...))` and relative paths that leave a package root; `scope.test.mjs` fails
// when a new read of that kind is not named here.
// A daemon test (`interface_language.rs`) parses the UI's language list.
export const DAEMON_READS_UI = ["packages/ui/src/i18n/languages.ts"];
// A UI test (`bundleLocalizations.test.ts`) compares the bundle's languages with the shell's `Info.plist`.
export const UI_READS_NATIVE = ["apps/desktop/src-tauri/Info.plist"];
// A UI test (`appConfig.test.ts`) holds `config/app.json` to the license and the default website address.
export const UI_READS_ELSEWHERE = ["LICENSE", "apps/web/site.config.ts"];
// A verify test (`rerun.test.mjs`) reads the reporter that shares `REPORT_ENV` with `rerun.mjs`.
export const VERIFY_READS_UI = ["packages/ui/src/test/failedFilesReporter.ts"];

export const ALL_PACKAGES = ["verify", "ui", "web", "desktop", "daemon", "native"];
const JS_PACKAGES = ["verify", "ui", "web", "desktop"];
const RUST_PACKAGES = ["daemon", "native"];

// `match` entries are a prefix (ending in "/"), an exact path or a RegExp. A source rule names its own
// directory with a pattern that leaves Markdown out, since documentation is read by no check.
const notMarkdown = (prefix) => new RegExp(`^${prefix.replaceAll("/", "\\/")}(?!.*\\.md$)`);

export const RULES = [
  { match: ["scripts/verify/", "scripts/verify.mjs", "scripts/format-staged.mjs", "scripts/pre-push.mjs"], consumers: ["verify"], why: "the verification command, the hook scripts and their tests" },
  { match: [notMarkdown("packages/ui/")], consumers: ["ui"], why: "the UI package" },
  { match: [notMarkdown("apps/web/")], consumers: ["web"], why: "the website" },
  { match: [notMarkdown("apps/daemon/")], consumers: ["daemon"], why: "the daemon crate" },
  { match: [notMarkdown("apps/desktop/src-tauri/")], consumers: ["native"], why: "the desktop shell crate" },
  { match: ["apps/desktop/scripts/"], consumers: ["desktop"], why: "the desktop build and release scripts and their tests" },
  { match: ["apps/desktop/package.json"], consumers: ["desktop", "native"], why: "its scripts are the desktop scripts' commands, and `tauri.conf.json` runs them as the shell's build commands" },

  // Cargo features unify across workspace members, so a manifest of either crate changes how both build.
  { match: ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "rustfmt.toml", ".cargo/", /\/Cargo\.toml$/], consumers: RUST_PACKAGES, why: "the Cargo workspace, its feature set and toolchain are shared by both crates" },
  { match: ["pnpm-lock.yaml", "pnpm-workspace.yaml", "patches/", "eslint.config.mjs"], consumers: JS_PACKAGES, why: "dependency versions and the lint rules apply to every JavaScript package" },
  { match: ["package.json"], consumers: ALL_PACKAGES, why: "holds the scripts of every package, including `test:rust`, and the shared dev dependencies" },
  { match: ["scripts/env.mjs", "scripts/env.d.mts"], consumers: ["verify", "ui", "desktop"], why: "imported by `scripts/verify/lock.mjs`, `packages/ui/vite.config.ts` (its declaration types that import) and the desktop release scripts" },
  { match: ["scripts/test-rust.mjs"], consumers: ["verify", ...RUST_PACKAGES], why: "the command the Rust test step runs, linted with the scripts" },
  { match: [".githooks/"], consumers: ["verify"], why: "the hook entry point is exercised by the pre-push tests; nothing else reads it" },
  { match: [".gitignore"], consumers: ["verify"], why: "decides which files the working-tree snapshot (`tree.mjs`) counts; no other check reads it" },
  { match: ["config/"], consumers: ALL_PACKAGES, builds: ["ui"], why: "`config/app.json` is the product identity every package reads, and `vite.config.ts` bakes it into the UI build" },

  // Nothing local reads these. Listed so that they do not fall through to "unknown".
  { match: ["docs/", ".claude/", ".agents/", ".codex/", "assets/branding/", ".github/", "CLAUDE.md", "AGENTS.md"], consumers: [], why: "documentation, agent files, brand assets and CI definitions are read by no local check" },

  // Cross-package reads. Each also selects `verify`, whose tests check that the paths named here exist.
  { match: [...WEB_IMPORTS_FROM_UI, ...WEB_PREVIEW_INPUTS], consumers: ["verify", "ui", "web"], why: "the website imports these modules, or builds its preview from them" },
  { match: WIRE_PROTOCOL, consumers: ["verify", "ui", "daemon", "native"], why: "the wire protocol is kept in step by hand across the daemon, the UI and the desktop shell" },
  { match: DESKTOP_CONTRACT, consumers: ["verify", "ui", "native"], why: "the UI side of the contract with the desktop shell" },
  { match: SWEEP_SHARED, consumers: ["verify", "ui"], why: "the UI's gallery sweep, which its tests run, imports it" },
  { match: DAEMON_READS_UI, consumers: ["verify", "ui", "daemon"], why: "a daemon test parses it" },
  { match: UI_READS_NATIVE, consumers: ["verify", "ui", "native"], why: "a UI test reads it" },
  { match: UI_READS_ELSEWHERE, consumers: ["verify", "ui"], why: "a UI test reads it" },
  { match: VERIFY_READS_UI, consumers: ["verify", "ui"], why: "a verify test reads it" },
  { match: ["apps/daemon/src/adapter/claude_compatibility.md"], consumers: ["daemon"], why: "compiled into the daemon with `include_str!`" },

  // The UI build runs only for changes to what it reads besides `src/`. The gallery and benchmark
  // scripts and pages are not among them, and the website's preview build has no step of its own.
  { match: ["packages/ui/vite.config.ts", "packages/ui/index.html", "packages/ui/typography.mjs", "packages/ui/scripts/generate-licenses.mjs", "packages/ui/package.json", "packages/ui/tsconfig.json", "packages/ui/tsconfig.app.json", "Cargo.lock", "pnpm-lock.yaml", "scripts/env.mjs"], consumers: [], builds: ["ui"], why: "the inputs of `pnpm build` of the UI: the bundler configuration, the page, and the lock files and manifests the license generator reads" },
];

const matches = (matcher, file) => matcher instanceof RegExp ? matcher.test(file) : matcher.endsWith("/") ? file.startsWith(matcher) : file === matcher;

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
