# Adding a dependency

## Anything the UI bundle ships goes into `dependencies` of `packages/ui`, never `devDependencies`

Applies to every npm package whose output ends up in the built UI, not only libraries the source imports: a
stylesheet a CSS file `@import`s by package name, a Tailwind plugin, anything a plugin module such as
`packages/ui/typography.mjs` imports. Vite bundles a package whichever field lists it, so a misplaced one builds and
runs fine, but the third-party licenses list Settings > About shows is generated from the production dependency
closure of `packages/ui` only (`packages/ui/scripts/generate-licenses.mjs`), and a dev dependency ships without its
license listed. `packages/ui/src/cssDependencies.test.ts` catches a misplaced package the stylesheets name; nothing
catches one the TypeScript source imports. Build-only tooling (Vite and its plugins, compilers, test tools, `@types/*`)
stays in `devDependencies`.
