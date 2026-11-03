// The import architecture of `packages/ui/src`, as `no-restricted-imports` config blocks that the root
// `eslint.config.mjs` spreads in after the UI's other rules (a later block's options replace an earlier
// one's). Which directory may import which is described in docs/structure/ui.md.

// ESLint replaces a rule's options with those of the last block that matches a file and sets it, so every UI
// block below restates the Alert restrictions through `uiImports`.
const ALERT_MESSAGE = "Use ErrorAlert, or FailureAlert or ChoiceWarning built on it, which require a title.";
const ALERT_PATHS = [
  {
    name: "@heroui/react",
    importNames: ["Alert", "AlertRoot", "AlertIndicator", "AlertContent", "AlertTitle", "AlertDescription"],
    message: ALERT_MESSAGE,
  },
  { name: "@heroui/react/alert", message: ALERT_MESSAGE },
];
const ALERT_FRAME = {
  group: ["**/AlertFrame"],
  message: "AlertFrame draws an Alert with no title; use ErrorAlert, FailureAlert or ChoiceWarning.",
};
const uiImports = (patterns = [], { alerts = true } = {}) =>
  ["error", alerts ? { paths: ALERT_PATHS, patterns: [ALERT_FRAME, ...patterns] } : { patterns }];

const UI_SRC = "packages/ui/src";
const UI_TESTS = ["**/*.test.{ts,tsx}", "**/*{TestSupport,testSupport,TestState}.{ts,tsx}"];
// The rule sees only the import's text, not the file it resolves to, so each block is for files at one depth
// below src/: there exactly `depth` steps of `../` reach src/, and the name after them is a top-level directory
// (followed by `/`) or a file at src/'s root. A name after fewer steps is a sibling inside the same directory,
// such as `../shell/` from `terminal/links/`, which a gitignore pattern like `**/shell/**` would also match.
const DEPTHS = [1, 2, 3, 4];
const below = (dir, depth) => `${UI_SRC}/${dir}/${"*/".repeat(depth - 1)}*.{ts,tsx}`;
const onlyInto = (depth, allowed, message) =>
  ({ regex: `^(?:\\.\\./){${depth}}(?!(?:${allowed.join("|")})/)[^/]+/`, message });
const notInto = (depth, denied, message) =>
  ({ regex: `^(?:\\.\\./){${depth}}(?:${denied.join("|")})(?:/|$)`, message });
// What code below src/ may import from src/'s root: the daemon contract, the app's configuration mirror, and the
// barrels of store/ and platform/ (a directory with an index, imported without a trailing slash). The rest of the root
// (the entry, App, the screens, style.css) is imported by nothing below src/.
const ROOT_SHARED = ["protocol", "daemon", "daemon-client", "appConfig", "store", "platform"];
const ROOT_MESSAGE = "Below src/, only protocol, daemon, daemon-client, appConfig and the store and platform barrels are imported from src/'s root.";
const onlyRoot = (depth, message, roots = ROOT_SHARED) =>
  ({ regex: `^(?:\\.\\./){${depth}}(?!(?:${roots.join("|")})$)[^/]+$`, message });
/** Outside itself, `dir` imports only the `allowed` top-level directories and the `roots` files at src/'s root. */
const layer = (dir, allowed, message, roots = ROOT_SHARED) => DEPTHS.map((depth) => ({
  files: [below(dir, depth)],
  ignores: UI_TESTS,
  rules: { "no-restricted-imports": uiImports([onlyInto(depth, allowed, message), onlyRoot(depth, message, roots)]) },
}));
// The pure layers, which import no component, no store and no feature directory.
const PURE_LAYERS = ["i18n", "platform", "preferences", "input", "format", "agents", "git", "sessions", "errors", "browse"];
const PURE_MESSAGE = "the pure layers (i18n/, platform/, preferences/, input/, format/, agents/, git/, sessions/, errors/, browse/)";
const LEAF_MESSAGE = `components/ imports, outside itself, only ${PURE_MESSAGE} and the shared root files.`;
const SHELL_MESSAGE = "Only the files at src/'s root and App's hooks put the window shell together; nothing else below src/ imports shell/.";
const APP_MESSAGE = "app/ holds App's own hooks and logic: only App.tsx and the tests import it.";
// Case-sensitive, as the patterns match case-insensitively by default and `./App` is not app/.
const APP_ROOT_IMPORT = { regex: "^\\./app(?:/|$)", message: APP_MESSAGE, caseSensitive: true };
const PRODUCTION_MESSAGE = "Production code does not import fixtures/ or gallery/: they are the runtime of the gallery and the website preview.";
const GALLERY_MESSAGE = "gallery/ is the dev-only gallery: the website preview and fixtures/ depend on nothing in it.";
const ROOT_PRODUCTION_IMPORT = { regex: "^\\./(?:fixtures|gallery)(?:/|$)", message: PRODUCTION_MESSAGE };
const notIntoApp = (depth) => ({ ...notInto(depth, ["app"], APP_MESSAGE), caseSensitive: true });

export default [
  {
    files: ["packages/ui/**/*.{ts,tsx}"],
    // Every Alert has a title. ErrorAlert (exempted below, with AlertFrame) requires it in its types.
    rules: { "no-restricted-imports": uiImports() },
  },
  // Window shell: every directory below src/ but the shell itself, App's own hooks (app/) and the dev-only pages
  // leaves shell/ to the files at src/'s root. App's hooks: app/ is App's own layer and imports what App.tsx does,
  // and nothing but App.tsx and the tests imports it. The directories with a rule of their own below forbid both
  // by their whitelist.
  // Production code (everything but the gallery, the shared fixtures, the website preview, test support and the
  // tests) imports neither fixtures/ nor gallery/; the fixtures are the runtime the gallery and the preview share.
  ...DEPTHS.map((depth) => ({
    files: [below("*", depth)],
    ignores: [...UI_TESTS, `${UI_SRC}/{shell,app,gallery,fixtures,website-preview,test}/**`],
    rules: { "no-restricted-imports": uiImports([notInto(depth, ["shell"], SHELL_MESSAGE), notIntoApp(depth), notInto(depth, ["fixtures", "gallery"], PRODUCTION_MESSAGE), onlyRoot(depth, ROOT_MESSAGE)]) },
  })),
  ...DEPTHS.map((depth) => ({
    files: [below("shell", depth)],
    ignores: UI_TESTS,
    rules: { "no-restricted-imports": uiImports([notIntoApp(depth), notInto(depth, ["fixtures", "gallery"], PRODUCTION_MESSAGE)]) },
  })),
  ...DEPTHS.map((depth) => ({
    files: [below("app", depth)],
    ignores: UI_TESTS,
    rules: { "no-restricted-imports": uiImports([notInto(depth, ["fixtures", "gallery"], PRODUCTION_MESSAGE)]) },
  })),
  ...DEPTHS.map((depth) => ({
    files: [below("{gallery,test}", depth)],
    ignores: UI_TESTS,
    rules: { "no-restricted-imports": uiImports([notIntoApp(depth)]) },
  })),
  // The fixtures and the preview do not depend on the dev-only gallery. The fixtures keep the window shell's rule.
  ...DEPTHS.map((depth) => ({
    files: [below("website-preview", depth)],
    ignores: UI_TESTS,
    rules: { "no-restricted-imports": uiImports([notIntoApp(depth), notInto(depth, ["gallery"], GALLERY_MESSAGE)]) },
  })),
  ...DEPTHS.map((depth) => ({
    files: [below("fixtures", depth)],
    ignores: UI_TESTS,
    rules: { "no-restricted-imports": uiImports([notInto(depth, ["shell"], SHELL_MESSAGE), notIntoApp(depth), notInto(depth, ["gallery"], GALLERY_MESSAGE)]) },
  })),
  {
    files: [`${UI_SRC}/*.{ts,tsx}`],
    ignores: [...UI_TESTS, `${UI_SRC}/App.tsx`],
    rules: { "no-restricted-imports": uiImports([APP_ROOT_IMPORT, ROOT_PRODUCTION_IMPORT]) },
  },
  {
    // App.tsx is the one root file that may import app/, and is production code like the rest.
    files: [`${UI_SRC}/App.tsx`],
    rules: { "no-restricted-imports": uiImports([ROOT_PRODUCTION_IMPORT]) },
  },
  // Leaf layer: shared building blocks depend on no feature directory.
  ...layer("components", PURE_LAYERS, LEAF_MESSAGE),
  // Domain layers: each sits below every feature, and imports only the layers beneath it, plus the root files it lists.
  ...layer("platform", [], "platform/ imports nothing else below src/, apart from protocol.", ["protocol"]),
  ...layer("preferences", [], "preferences/ imports nothing else below src/.", []),
  ...layer("input", [], "input/ imports nothing else below src/, apart from the platform barrel.", ["platform"]),
  ...layer("format", ["i18n"], "format/ imports, outside itself, only i18n/.", []),
  ...layer("agents", ["i18n"], "agents/ imports, outside itself, only i18n/ and protocol.", ["protocol"]),
  ...layer("git", ["i18n"], "git/ imports, outside itself, only i18n/ and protocol.", ["protocol"]),
  ...layer("sessions", ["i18n", "agents"], "sessions/ imports, outside itself, only i18n/, agents/ and protocol.", ["protocol"]),
  ...layer("accounts", ["i18n", "agents", "sessions", "format"], "accounts/ imports, outside itself, only i18n/, agents/, sessions/, format/ and protocol.", ["protocol"]),
  ...layer("errors", ["i18n", "agents", "sessions", "accounts", "format"], "errors/ imports, outside itself, only i18n/, agents/, sessions/, accounts/, format/, protocol, appConfig and daemon-client.", ["protocol", "appConfig", "daemon-client"]),
  ...layer("browse", ["i18n", "errors", "format"], "browse/ imports, outside itself, only i18n/, errors/, format/, protocol and daemon-client.", ["protocol", "daemon-client"]),
  ...layer("store", ["i18n", "errors", "accounts", "sessions", "preferences", "git"], "store/ imports, outside itself, only i18n/, errors/, accounts/, sessions/, preferences/, git/, protocol, daemon and daemon-client.", ["protocol", "daemon", "daemon-client"]),
  ...layer("i18n", ["preferences"], "i18n/ imports, outside itself, only preferences/ and appConfig.", ["appConfig"]),
  ...layer("shortcuts", ["i18n", "preferences", "input", "components"], "shortcuts/ imports, outside itself, only i18n/, preferences/, input/ and components/.", []),
  // Self-contained features: they use the leaf layer and import no other feature directory.
  ...layer("viewer", ["components", ...PURE_LAYERS], `viewer/ imports, outside itself, only components/, ${PURE_MESSAGE}, and the shared root files.`),
  ...layer("about", ["components", ...PURE_LAYERS], `about/ imports, outside itself, only components/ and ${PURE_MESSAGE}.`, []),
  ...layer("updates", ["components", ...PURE_LAYERS], `updates/ imports, outside itself, only components/ and ${PURE_MESSAGE}, apart from the platform barrel.`, ["platform"]),
  ...layer("layout", ["components", ...PURE_LAYERS], `layout/ imports, outside itself, only components/ and ${PURE_MESSAGE}, apart from protocol.`, ["protocol"]),
  ...layer("quota", ["components", "accounts", ...PURE_LAYERS], `quota/ imports, outside itself, only components/, accounts/, ${PURE_MESSAGE}, and the shared root files.`),
  // The project browser builds on the viewer and on no other feature.
  ...layer("project-browser", ["components", "viewer", ...PURE_LAYERS], `project-browser/ imports, outside itself, only components/, viewer/, ${PURE_MESSAGE}, and the shared root files.`),
  {
    // ErrorAlert composes HeroUI's Alert through AlertFrame; both stay in the leaf layer.
    files: [`${UI_SRC}/components/alerts/AlertFrame.tsx`, `${UI_SRC}/components/alerts/ErrorAlert.tsx`],
    rules: { "no-restricted-imports": uiImports([onlyInto(2, PURE_LAYERS, LEAF_MESSAGE), onlyRoot(2, LEAF_MESSAGE)], { alerts: false }) },
  },
  {
    // Its test compares ErrorAlert with the stock Alert.
    files: [`${UI_SRC}/components/alerts/ErrorAlert.test.tsx`],
    rules: { "no-restricted-imports": "off" },
  },
];
