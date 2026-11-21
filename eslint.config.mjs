import js from "@eslint/js";
import stylistic from "@stylistic/eslint-plugin";
import { defineConfig, globalIgnores } from "eslint/config";
import reactHooks from "eslint-plugin-react-hooks";
import vue from "eslint-plugin-vue";
import globals from "globals";
import tseslint from "typescript-eslint";
import uiLayers, { LAZY_MESSAGE, UI_TESTS } from "./packages/ui/eslint.layers.mjs";
import lucide from "./scripts/eslint/lucide-aria-hidden.mjs";
import nativeTitle from "./scripts/eslint/native-title.mjs";
import primaryModifier from "./scripts/eslint/primary-modifier.mjs";
import uiClassNames from "./scripts/eslint/ui-class-names.mjs";

const javascript = ["**/*.{js,mjs,cjs}"];
const typescript = ["**/*.{ts,tsx,mts,cts}", "**/*.vue"];
const source = [...javascript, ...typescript];

const NULLISH_ASSIGNMENT = { selector: "AssignmentExpression[operator='??=']", message: "The React Compiler cannot lower `??=`: write `x = x ?? y`, an `if`, or a getter for a module-level value made on first use (docs/conventions/ui-code.md, \"The React Compiler\")." };

const unusedVariables = ["error", { argsIgnorePattern: "^_", varsIgnorePattern: "^_", caughtErrorsIgnorePattern: "^_" }];

export default defineConfig(
  globalIgnores([
    "**/node_modules/**",
    "**/.worktrees/**",
    "**/.pnpm-store/**",
    "**/.nuxt/**",
    "**/.astro/**",
    "**/.output/**",
    "apps/web/public/preview/**",
    "**/dist/**",
    "**/dist-preview/**",
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
  { files: javascript, extends: [js.configs.recommended], rules: { "no-unused-vars": unusedVariables } },
  {
    files: typescript,
    extends: [js.configs.recommended, tseslint.configs.recommended],
    rules: {
      // TypeScript resolves types and Nuxt auto-imports during the separate type check.
      "no-undef": "off",
      "@typescript-eslint/no-unused-vars": unusedVariables,
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
    // The React Compiler cannot lower `??=`, so a component or hook holding one is left uncompiled
    // (`docs/conventions/ui-code.md`, "The React Compiler"). Tests and test support may keep it: they are not what
    // users run.
    files: ["packages/ui/src/**/*.{ts,tsx}"],
    ignores: [...UI_TESTS, "packages/ui/src/test/**"],
    rules: {
      "no-restricted-syntax": ["error", NULLISH_ASSIGNMENT],
    },
  },
  ...uiLayers,
  // A lucide-react icon carries `aria-hidden` itself (see "Names, roles and states" in
  // docs/conventions/ui-accessibility.md).
  { files: ["packages/ui/**/*.{ts,tsx}"], plugins: { lucide }, rules: { "lucide/lucide-aria-hidden": "error" } },
  {
    // The UI's rules about class names, the native `title`, the Cmd/Ctrl test and `navigator`
    // (docs/conventions/ui-standard.md, ui-accessibility.md, ui-components.md, ui-code.md). Tests assert on class names
    // and stub the browser, and the gallery, the shared fixtures and the website preview are not application source
    // (their scenario text is prose).
    files: ["packages/ui/src/**/*.{ts,tsx}"],
    ignores: [...UI_TESTS, "packages/ui/src/{test,gallery,fixtures,website-preview}/**"],
    plugins: { "ui-class-names": uiClassNames, "native-title": nativeTitle, "primary-modifier": primaryModifier },
    rules: {
      "ui-class-names/no-focus-visible": "error",
      "ui-class-names/no-ellipsis": "error",
      "ui-class-names/no-physical-direction": "error",
      "ui-class-names/no-border-divider": "error",
      "native-title/native-title": "error",
      "primary-modifier/primary-modifier": "error",
      // A later block's options replace an earlier one's, so the `??=` selector above is restated here.
      "no-restricted-syntax": ["error", NULLISH_ASSIGNMENT,
        {
          selector: "MemberExpression[computed=false][property.name=/^(?:userAgent|platform|userAgentData)$/]:matches([object.name='navigator'], [object.object.name=/^(?:window|globalThis|self)$/][object.property.name='navigator'])",
          message: "Behaviour or words that depend on the operating system read the host: `usePlatform()` / `useHost()` for the client's, `useHostPaths()` for the daemon machine's (docs/conventions/ui-code.md, \"Behaviour or words that depend on the operating system read the host, never `navigator`\").",
        },
        { selector: "MemberExpression[computed=false][object.name='React'][property.name='lazy']", message: LAZY_MESSAGE },
        {
          selector: "ImportDeclaration[source.value='lucide-react'] > ImportSpecifier[imported.name=/^Loader/]",
          message: "A wait is drawn with the layer's `Spinner`, never a lucide loader glyph (docs/conventions/ui-standard.md, \"Icon vocabulary\").",
        },
      ],
    },
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
