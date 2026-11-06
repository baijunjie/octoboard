# UI Component Wrapper Layer Development Plan

## Problem and approach

Application code imports `@heroui/react` and `react-aria-components` directly, both from about 45 business modules and
from about 35 shared components. A treatment that every instance of a component needs is therefore applied per call
site, and call sites get missed: the WKWebView pixel shift on tabs was fixed for the Settings sidebar only and came
back in the other tab strips, until a global `.tabs__tab` rule in the stylesheet covered them all.

The UI standard is changed so that every third-party UI component reaches the application through one project-owned
wrapper layer, whether or not the project customises it. Business code, and the project's own composite components,
use only the wrappers. Lint enforces the boundary.

## Key design decisions

- **Scope.** Components from `@heroui/react` and `react-aria-components` are wrapped. `react-aria` hooks and
  interaction utilities (`useFocusRing`, `useHover`, `setInteractionModality` and the like) may be used only inside the
  wrapper layer. Libraries that are not UI component libraries, such as xterm and react-markdown, are out of scope.
- **One dedicated layer.** The wrappers live in a single layer of their own, one wrapper per third-party component.
  Only that layer may import the third-party libraries. The project's composite components outside it (option
  pickers, menus, form sections and the like) are consumers like business code and use the wrappers too, so a
  component is customised in exactly one place.
- **Project-defined APIs.** Each wrapper defines its own props and exposes only what the project uses; third-party
  types and compound APIs are not passed through. This keeps the third-party API out of application code, so a later
  change of component library touches only the layer. Every call site is rewritten against the new API.
- **Appearance is unchanged.** The existing UI standard still applies: wrappers keep HeroUI's default appearance
  through its variants and slots, and add only the treatments the standard already approves. Moving a treatment into a
  wrapper is not a licence for new styling.
- **Shared fixes move into their wrapper.** The global `.tabs__tab` compositing rule moves into the Tab wrapper, so
  the stylesheet no longer carries a per-component fix that a wrapper owns.
- **Enforcement ratchets.** The import rule is turned on in the first milestone, with an allowlist of the files not
  yet migrated; each milestone removes the files it migrates from the allowlist, and the last one removes the
  allowlist. New code is held to the rule from the start.
- **Milestones go by component family.** Each family's wrappers are designed and all its call sites migrated in one
  milestone, so each milestone leaves that family fully behind the layer and can be merged on its own.

## Milestones

1. Layer foundation, standard and lint (closed)
2. Tabs (closed)
3. Buttons and toggles (closed)
4. Form fields (closed)
5. Overlays (closed)
6. Display components (closed)
7. Collections, focus and interaction (closed)
8. Close the boundary (closed)
9. [Final confirmation](09-final-confirmation.md)

## Settled scope details

- **The layer's place.** `packages/ui/src/primitives/`, with one entry point: the lowest layer of the UI package, below
  the pure layers, so every directory may import it, while it imports only the third-party libraries and nothing else
  of the application. What a wrapper needs from the application (translated labels and the like) comes through props
  or a context the application provides.
- **`@heroui/styles` counts as wrapped**, like `@heroui/react`; **the website preview** is exempt from the rule like
  the shared fixtures.
- **Tests, the gallery and the shared fixtures** may import the third-party libraries directly (for example
  `setInteractionModality` to set the input modality in tests); the rule covers application source only.
- **React Aria utilities that are neither components nor hooks** (`mergeProps`, `mergeRefs`, `parseColor`, collection
  key types and the like) are inside the boundary: the layer re-exports the ones the application uses.
- **HeroUI's stylesheet** stays imported by the application stylesheet; the theme hook (`useTheme`) is wrapped by the
  layer like any other hook.
