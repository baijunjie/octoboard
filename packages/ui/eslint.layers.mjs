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
// The wrapper layer's alert parts are the same Alert, for AlertFrame and ErrorAlert alone.
const ALERT_WRAPPER = {
  group: ["**/primitives"],
  importNames: ["Alert", "AlertIndicator", "AlertContent", "AlertTitle", "AlertDescription"],
  message: ALERT_MESSAGE,
};
const ALERT_FRAME = {
  group: ["**/AlertFrame"],
  message: "AlertFrame draws an Alert with no title; use ErrorAlert, FailureAlert or ChoiceWarning.",
};
// The generated wire types are read through protocol.ts, the one file that imports them, so that the UI has one door to
// the wire and a type added there (the two `Client*` unions) is never bypassed. The rule sees import declarations only,
// not a type written as `import("…/protocol.generated").X`.
const GENERATED_PROTOCOL = {
  group: ["**/protocol.generated"],
  message: "Import the wire types from protocol, which re-exports protocol.generated.",
};
const uiImports = (patterns = [], { alerts = true, generated = true } = {}) => {
  const all = generated ? [GENERATED_PROTOCOL, ...patterns] : patterns;
  return ["error", alerts ? { paths: ALERT_PATHS, patterns: [ALERT_WRAPPER, ALERT_FRAME, ...all] } : { patterns: all }];
};

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
// The bottom layer: the wrapper layer over the third-party UI libraries, which every other directory may import, by its
// barrel (a directory with an index, imported without a trailing slash, as the rule sees a root file).
const PRIMITIVES = "primitives";
const PRIMITIVES_NOTE = "Every layer may also import primitives, the wrapper layer.";
// The barrel only: a layer block's whitelist already refuses a file inside primitives/, and the blocks without one say so
// here. `primitives/` followed by a path, so the barrel itself, imported as `primitives`, stays allowed.
const PRIMITIVES_FILE_MESSAGE = "Import the wrapper layer from its barrel, primitives, never a file inside it.";
const primitivesFile = (depth) => ({ regex: `^(?:\\.\\./){${depth}}${PRIMITIVES}/.`, message: PRIMITIVES_FILE_MESSAGE });
const ROOT_PRIMITIVES_FILE = { regex: `^\\./${PRIMITIVES}/.`, message: PRIMITIVES_FILE_MESSAGE };
// What code below src/ may import from src/'s root: the daemon contract, the app's configuration mirror, and the
// barrels of store/, platform/ and primitives/. The rest of the root (the entry, App, the screens, style.css) is
// imported by nothing below src/.
const ROOT_SHARED = ["protocol", "requests", "daemon", "daemon-client", "appConfig", "store", "platform", PRIMITIVES];
const ROOT_MESSAGE = "Below src/, only protocol, requests, daemon, daemon-client, appConfig and the store, platform and primitives barrels are imported from src/'s root.";
const onlyRoot = (depth, message, roots = ROOT_SHARED) =>
  ({ regex: `^(?:\\.\\./){${depth}}(?!(?:${roots.join("|")})$)[^/]+$`, message });
/** Outside itself, `dir` imports only the `allowed` top-level directories, the `roots` files at src/'s root, and the
 * barrel of primitives/. */
const layer = (dir, allowed, message, roots = ROOT_SHARED) => DEPTHS.map((depth) => ({
  files: [below(dir, depth)],
  ignores: UI_TESTS,
  rules: { "no-restricted-imports": uiImports([onlyInto(depth, allowed, message), onlyRoot(depth, `${message} ${PRIMITIVES_NOTE}`, [PRIMITIVES, ...roots])]) },
}));
// The pure layers, which import no component, no store and no feature directory.
const PURE_LAYERS = ["values", "providers", "i18n", "platform", "preferences", "input", "format", "agents", "git", "sessions", "errors", "browse"];
const PURE_MESSAGE = "the pure layers (values/, providers/, i18n/, platform/, preferences/, input/, format/, agents/, git/, sessions/, errors/, browse/)";
const LEAF_MESSAGE = `components/ imports, outside itself, only ${PURE_MESSAGE} and the shared root files.`;
const SHELL_MESSAGE = "Only the files at src/'s root and App's hooks put the window shell together; nothing else below src/ imports shell/.";
const APP_MESSAGE = "app/ holds App's own hooks and logic: only App.tsx and the tests import it.";
// Case-sensitive, as the patterns match case-insensitively by default and `./App` is not app/.
const APP_ROOT_IMPORT = { regex: "^\\./app(?:/|$)", message: APP_MESSAGE, caseSensitive: true };
const PRODUCTION_MESSAGE = "Production code does not import fixtures/ or gallery/: they are the runtime of the gallery and the website preview.";
const GALLERY_MESSAGE = "gallery/ is the dev-only gallery: the website preview and fixtures/ depend on nothing in it.";
const PROTOCOL_MESSAGE = "protocol.ts imports only protocol.generated, the wire types it is the one door to.";
const ROOT_PRODUCTION_IMPORT = { regex: "^\\./(?:fixtures|gallery)(?:/|$)", message: PRODUCTION_MESSAGE };
const notIntoApp = (depth) => ({ ...notInto(depth, ["app"], APP_MESSAGE), caseSensitive: true });

// The third-party UI libraries that reach the application only through the wrapper layer, `src/primitives/`
// (docs/ui-standard.md, "Third-party UI components come through the wrapper layer"). The rule is typescript-eslint's
// `no-restricted-imports`, not the core one the blocks above replace among themselves, so it holds alongside them. Like
// them it sees import and export declarations, not `import("…")` calls or type expressions. Tests and test support, the
// gallery, the shared fixtures and the website preview are not application source and may import the libraries.
const WRAPPED_LIBRARIES = [
  "@heroui/react", "@heroui/react/*", "@heroui/styles", "@heroui/styles/*",
  "react-aria-components", "react-aria-components/*", "react-aria", "react-aria/*",
  "@react-aria/*", "@react-stately/*", "@react-types/*",
];
const WRAPPED_MESSAGE = "Third-party UI components, hooks and utilities reach the application only through the wrapper layer: import them from primitives, adding a wrapper there if it lacks one.";

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
    rules: { "no-restricted-imports": uiImports([notInto(depth, ["shell"], SHELL_MESSAGE), notIntoApp(depth), notInto(depth, ["fixtures", "gallery"], PRODUCTION_MESSAGE), onlyRoot(depth, ROOT_MESSAGE), primitivesFile(depth)]) },
  })),
  ...DEPTHS.map((depth) => ({
    files: [below("shell", depth)],
    ignores: UI_TESTS,
    rules: { "no-restricted-imports": uiImports([notIntoApp(depth), notInto(depth, ["fixtures", "gallery"], PRODUCTION_MESSAGE), primitivesFile(depth)]) },
  })),
  ...DEPTHS.map((depth) => ({
    files: [below("app", depth)],
    ignores: UI_TESTS,
    rules: { "no-restricted-imports": uiImports([notInto(depth, ["fixtures", "gallery"], PRODUCTION_MESSAGE), primitivesFile(depth)]) },
  })),
  ...DEPTHS.map((depth) => ({
    files: [below("{gallery,test}", depth)],
    ignores: UI_TESTS,
    rules: { "no-restricted-imports": uiImports([notIntoApp(depth), primitivesFile(depth)]) },
  })),
  // The fixtures and the preview do not depend on the dev-only gallery. The fixtures keep the window shell's rule.
  ...DEPTHS.map((depth) => ({
    files: [below("website-preview", depth)],
    ignores: UI_TESTS,
    rules: { "no-restricted-imports": uiImports([notIntoApp(depth), notInto(depth, ["gallery"], GALLERY_MESSAGE), primitivesFile(depth)]) },
  })),
  ...DEPTHS.map((depth) => ({
    files: [below("fixtures", depth)],
    ignores: UI_TESTS,
    rules: { "no-restricted-imports": uiImports([notInto(depth, ["shell"], SHELL_MESSAGE), notIntoApp(depth), notInto(depth, ["gallery"], GALLERY_MESSAGE), primitivesFile(depth)]) },
  })),
  {
    files: [`${UI_SRC}/*.{ts,tsx}`],
    ignores: [...UI_TESTS, `${UI_SRC}/App.tsx`],
    rules: { "no-restricted-imports": uiImports([APP_ROOT_IMPORT, ROOT_PRODUCTION_IMPORT, ROOT_PRIMITIVES_FILE]) },
  },
  {
    // protocol.ts is the one door to the generated wire types and holds nothing else of the UI.
    files: [`${UI_SRC}/protocol.ts`],
    rules: { "no-restricted-imports": uiImports([{ regex: "^\\.\\.?/(?!protocol\\.generated$)", message: PROTOCOL_MESSAGE }], { generated: false }) },
  },
  {
    // App.tsx is the one root file that may import app/, and is production code like the rest.
    files: [`${UI_SRC}/App.tsx`],
    rules: { "no-restricted-imports": uiImports([ROOT_PRODUCTION_IMPORT, ROOT_PRIMITIVES_FILE]) },
  },
  // The wrapper layer, below everything: it imports the third-party UI libraries and nothing else of the application.
  // Its Alert wraps HeroUI's.
  ...DEPTHS.map((depth) => ({
    files: [below(PRIMITIVES, depth)],
    ignores: UI_TESTS,
    rules: { "no-restricted-imports": uiImports([{ regex: `^(?:\\.\\./){${depth}}`, message: "primitives/ imports nothing else below src/: only the third-party UI libraries it wraps." }], { alerts: false }) },
  })),
  // Leaf layer: shared building blocks depend on no feature directory.
  ...layer("components", PURE_LAYERS, LEAF_MESSAGE),
  // Domain layers: each sits below every feature, and imports only the layers beneath it, plus the root files it lists.
  ...layer("values", [], "values/ imports nothing else below src/.", []),
  ...layer("providers", [], "providers/ imports nothing else below src/: the catalogs and apps/web import it.", []),
  ...layer("platform", [], "platform/ imports nothing else below src/, apart from protocol.", ["protocol"]),
  ...layer("preferences", [], "preferences/ imports nothing else below src/.", []),
  ...layer("input", [], "input/ imports nothing else below src/, apart from the platform barrel.", ["platform"]),
  ...layer("format", ["i18n"], "format/ imports, outside itself, only i18n/.", []),
  ...layer("agents", ["i18n", "providers"], "agents/ imports, outside itself, only i18n/, providers/ and protocol.", ["protocol"]),
  ...layer("git", ["i18n"], "git/ imports, outside itself, only i18n/ and protocol.", ["protocol"]),
  ...layer("sessions", ["i18n", "agents"], "sessions/ imports, outside itself, only i18n/, agents/ and protocol.", ["protocol"]),
  ...layer("accounts", ["i18n", "agents", "sessions", "format"], "accounts/ imports, outside itself, only i18n/, agents/, sessions/, format/ and protocol.", ["protocol"]),
  ...layer("errors", ["i18n", "agents", "sessions", "accounts", "format"], "errors/ imports, outside itself, only i18n/, agents/, sessions/, accounts/, format/, protocol and appConfig.", ["protocol", "appConfig"]),
  ...layer("browse", ["i18n", "errors", "format"], "browse/ imports, outside itself, only i18n/, errors/, format/ and protocol.", ["protocol"]),
  ...layer("store", ["values", "i18n", "agents", "errors", "accounts", "sessions", "preferences", "git"], "store/ imports, outside itself, only values/, i18n/, agents/, errors/, accounts/, sessions/, preferences/, git/, protocol, requests, daemon and daemon-client.", ["protocol", "requests", "daemon", "daemon-client"]),
  ...layer("i18n", ["preferences", "providers"], "i18n/ imports, outside itself, only preferences/, providers/ and appConfig.", ["appConfig"]),
  ...layer("shortcuts", ["i18n", "preferences", "input", "components"], "shortcuts/ imports, outside itself, only i18n/, preferences/, input/ and components/.", []),
  // Self-contained features: they use the leaf layer and import no other feature directory.
  ...layer("viewer", ["components", ...PURE_LAYERS], `viewer/ imports, outside itself, only components/, ${PURE_MESSAGE}, and the shared root files.`),
  ...layer("about", ["components", ...PURE_LAYERS], `about/ imports, outside itself, only components/ and ${PURE_MESSAGE}.`, []),
  ...layer("updates", ["components", ...PURE_LAYERS], `updates/ imports, outside itself, only components/ and ${PURE_MESSAGE}, apart from the platform barrel.`, ["platform"]),
  ...layer("layout", ["components", ...PURE_LAYERS], `layout/ imports, outside itself, only components/ and ${PURE_MESSAGE}, apart from protocol.`, ["protocol"]),
  ...layer("quota", ["components", "accounts", ...PURE_LAYERS], `quota/ imports, outside itself, only components/, accounts/, ${PURE_MESSAGE}, and the shared root files.`),
  ...layer("statusItem", ["components", "accounts", "quota", ...PURE_LAYERS], `statusItem/ imports, outside itself, only components/, accounts/, quota/, ${PURE_MESSAGE}, protocol and the store and platform barrels.`, ["protocol", "store", "platform"]),
  // The project browser builds on the viewer and on no other feature.
  ...layer("project-browser", ["components", "viewer", ...PURE_LAYERS], `project-browser/ imports, outside itself, only components/, viewer/, ${PURE_MESSAGE}, and the shared root files.`),
  {
    // ErrorAlert composes the wrapper layer's Alert through AlertFrame; both stay in the leaf layer.
    files: [`${UI_SRC}/components/alerts/AlertFrame.tsx`, `${UI_SRC}/components/alerts/ErrorAlert.tsx`],
    rules: { "no-restricted-imports": uiImports([onlyInto(2, PURE_LAYERS, LEAF_MESSAGE), onlyRoot(2, LEAF_MESSAGE)], { alerts: false }) },
  },
  {
    // Its test compares ErrorAlert with the stock Alert.
    files: [`${UI_SRC}/components/alerts/ErrorAlert.test.tsx`],
    rules: { "no-restricted-imports": "off" },
  },
  {
    files: [`${UI_SRC}/**/*.{ts,tsx}`],
    ignores: [...UI_TESTS, `${UI_SRC}/{primitives,gallery,fixtures,website-preview,test}/**`],
    rules: { "@typescript-eslint/no-restricted-imports": ["error", { patterns: [{ group: WRAPPED_LIBRARIES, message: WRAPPED_MESSAGE }] }] },
  },
];
