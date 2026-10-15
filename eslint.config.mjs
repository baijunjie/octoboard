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
