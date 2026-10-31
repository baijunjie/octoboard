import js from "@eslint/js";
import stylistic from "@stylistic/eslint-plugin";
import { defineConfig, globalIgnores } from "eslint/config";
import reactHooks from "eslint-plugin-react-hooks";
import vue from "eslint-plugin-vue";
import globals from "globals";
import tseslint from "typescript-eslint";

const javascript = ["**/*.{js,mjs,cjs}"];
const typescript = ["**/*.{ts,tsx,mts,cts}", "**/*.vue"];
const source = [...javascript, ...typescript];

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
// src/store/ is a module with an index, so it counts as a root file.
const DEPTHS = [1, 2, 3, 4];
const below = (dir, depth) => `${UI_SRC}/${dir}/${"*/".repeat(depth - 1)}*.{ts,tsx}`;
const onlyInto = (depth, allowed, message) =>
  ({ regex: `^(?:\\.\\./){${depth}}(?!(?:${allowed.join("|")})/)[^/]+/`, message });
const notInto = (depth, denied, message) =>
  ({ regex: `^(?:\\.\\./){${depth}}(?:${denied.join("|")})(?:/|$)`, message });
/** Outside itself, `dir` imports only the `allowed` top-level directories and files at src/'s root. */
const layer = (dir, allowed, message) => DEPTHS.map((depth) => ({
  files: [below(dir, depth)],
  ignores: UI_TESTS,
  rules: { "no-restricted-imports": uiImports([onlyInto(depth, allowed, message)]) },
}));
const LEAF = ["i18n", "platform", "sessions"];
const LEAF_MESSAGE = "components/ is the leaf layer: outside itself it imports only i18n/, platform/, sessions/ and files at src/'s root.";
const SHELL_MESSAGE = "Only the files at src/'s root and App's hooks put the window shell together; nothing else below src/ imports shell/.";
const APP_MESSAGE = "app/ holds App's own hooks: only App.tsx and the tests import it.";
// Case-sensitive, as the patterns match case-insensitively by default and `./App` is not app/.
const APP_ROOT_IMPORT = { regex: "^\\./app(?:/|$)", message: APP_MESSAGE, caseSensitive: true };
const PRODUCTION_MESSAGE = "Production code does not import fixtures/ or gallery/: they are the runtime of the gallery and the website preview.";
const GALLERY_MESSAGE = "gallery/ is the dev-only gallery: the website preview and fixtures/ depend on nothing in it.";
const ROOT_PRODUCTION_IMPORT = { regex: "^\\./(?:fixtures|gallery)(?:/|$)", message: PRODUCTION_MESSAGE };
const notIntoApp = (depth) => ({ ...notInto(depth, ["app"], APP_MESSAGE), caseSensitive: true });

export default defineConfig(
  globalIgnores([
    "**/node_modules/**",
    "**/.worktrees/**",
    "**/.pnpm-store/**",
    "**/.nuxt/**",
    "**/.output/**",
    "apps/web/public/preview/**",
    "**/dist/**",
    "**/build/**",
    "**/out/**",
    "**/coverage/**",
    "**/target/**",
    "packages/ui/.bench/**",
    // Written by a daemon test from the Rust types; its bytes are the test's to decide.
    "packages/ui/src/protocol.generated.ts",
    ".claude/**",
    ".agents/**",
    ".codex/**",
  ]),
  {
    files: source,
    languageOptions: { globals: globals.browser },
  },
  { files: ["scripts/**/*.mjs", "apps/*/scripts/**/*.mjs", "packages/*/scripts/**/*.mjs", "eslint.config.mjs"], languageOptions: { globals: globals.node } },
  { files: ["packages/ui/src/report/pageBridge.js"], languageOptions: { globals: { DOMPurify: "readonly" } } },
  { files: javascript, extends: [js.configs.recommended] },
  {
    files: typescript,
    extends: [js.configs.recommended, tseslint.configs.recommended],
    rules: {
      // TypeScript resolves types and Nuxt auto-imports during the separate type check.
      "no-undef": "off",
      "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_", varsIgnorePattern: "^_", caughtErrorsIgnorePattern: "^_" }],
    },
  },
  {
    files: ["**/*.vue"],
    extends: [vue.configs["flat/essential"]],
    languageOptions: { parserOptions: { parser: tseslint.parser } },
    rules: {
      "vue/html-indent": ["error", 2],
      "vue/html-quotes": ["error", "double"],
      "vue/html-end-tags": "error",
      "vue/no-spaces-around-equal-signs-in-attribute": "error",
    },
  },
  // Nuxt derives routes and its error entry point from these filenames.
  { files: ["apps/web/app/pages/**/*.vue", "apps/web/app/error.vue"], rules: { "vue/multi-word-component-names": "off" } },
  {
    files: ["packages/ui/**/*.{ts,tsx}"],
    // The React Compiler's rules, which flag what keeps a component or hook from being compiled.
    extends: [reactHooks.configs.flat.recommended],
    rules: {
      // An effect here lists the values it is to react to, not every value it reads: one that must
      // fire on a particular change depends on resolved ids and statuses rather than on the store
      // maps it reads them from, which every daemon event replaces. The rule stays off rather than
      // disabled per line at each such site because `babel-plugin-react-compiler` skips any function
      // carrying an `eslint-disable` for it or for `rules-of-hooks`, which would leave every such
      // component uncompiled.
      "react-hooks/exhaustive-deps": "off",
      // Lint-only: it has no bearing on what the compiler compiles. What it finds is state set from
      // an effect when an input changes, which moving into render would change what each commit shows.
      "react-hooks/set-state-in-effect": "off",
    },
  },
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
    rules: { "no-restricted-imports": uiImports([notInto(depth, ["shell"], SHELL_MESSAGE), notIntoApp(depth), notInto(depth, ["fixtures", "gallery"], PRODUCTION_MESSAGE)]) },
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
  ...layer("components", LEAF, LEAF_MESSAGE),
  // Domain layers: the session model and the shortcut registry sit below every feature.
  ...["sessions", "shortcuts"].flatMap((dir) =>
    layer(dir, ["i18n", "platform"], `${dir}/ imports, outside itself, only i18n/, platform/ and files at src/'s root.`)),
  // Self-contained features: they use the leaf layer and import no other feature directory.
  ...["viewer", "about", "updates", "quota"].flatMap((dir) =>
    layer(dir, ["components", ...LEAF], `${dir}/ imports no other feature directory.`)),
  // The project browser builds on the viewer and on no other feature.
  ...layer("project-browser", ["components", "viewer", ...LEAF], "project-browser/ imports, of the feature directories, only viewer/."),
  {
    // ErrorAlert composes HeroUI's Alert through AlertFrame; both stay in the leaf layer.
    files: [`${UI_SRC}/components/alerts/AlertFrame.tsx`, `${UI_SRC}/components/alerts/ErrorAlert.tsx`],
    rules: { "no-restricted-imports": uiImports([onlyInto(2, LEAF, LEAF_MESSAGE)], { alerts: false }) },
  },
  {
    // Its test compares ErrorAlert with the stock Alert.
    files: [`${UI_SRC}/components/alerts/ErrorAlert.test.tsx`],
    rules: { "no-restricted-imports": "off" },
  },
  {
    files: source,
    plugins: { "@stylistic": stylistic },
    rules: {
      "@stylistic/indent": ["error", 2, { SwitchCase: 1 }],
      "@stylistic/quotes": ["error", "double", { avoidEscape: true }],
      "@stylistic/semi": ["error", "always"],
      "@stylistic/comma-dangle": ["error", "always-multiline"],
      "@stylistic/comma-spacing": "error",
      "@stylistic/key-spacing": "error",
      "@stylistic/keyword-spacing": "error",
      "@stylistic/space-infix-ops": "error",
      "@stylistic/space-before-blocks": "error",
      "@stylistic/space-before-function-paren": ["error", { anonymous: "always", named: "never", asyncArrow: "always" }],
      "@stylistic/object-curly-spacing": ["error", "always"],
      "@stylistic/array-bracket-spacing": ["error", "never"],
      "@stylistic/arrow-spacing": "error",
      "@stylistic/arrow-parens": ["error", "always"],
      "@stylistic/brace-style": ["error", "1tbs", { allowSingleLine: true }],
      "@stylistic/no-trailing-spaces": "error",
      "@stylistic/eol-last": ["error", "always"],
      "@stylistic/jsx-quotes": ["error", "prefer-double"],
      "@stylistic/jsx-indent-props": ["error", 2],
    },
  },
);
