> Severity: Moderate

## Symptom

The embedded website workbench preview fails to initialize at phone width.

## Reproduction steps

1. Build the website preview with `pnpm --filter @octoboard/ui build:preview`.
2. Start the website development server and open `/zh/` in Chromium with a 369 by 900 pixel viewport.
3. Scroll to the product preview and wait for initialization.
4. Observe the localized failure message and the browser console error.

## Expected vs. actual

Expected: The overview scene initializes at phone width, as described in
`docs/product/website-product-preview.md`, "Product preview".

Actual: Initialization fails with `Website preview failed Error: Nothing to press matching 显示会话列表`.
The frame and its host show their localized failure messages.

## Environment

Revision: `28a80d2ab`; macOS; Chromium headless shell build 1243 through Playwright; Simplified Chinese;
369 by 900 pixel viewport; the generated preview assets are present and all five feature-card previews load.

## Scope of impact

Verified for the embedded Simplified Chinese overview scene at phone width. Other languages, scenes and desktop
widths were not checked for this defect. The five feature-card previews remain usable. A workbench workaround is
unknown.

## Leads

- Verified: `packages/ui/src/website-preview/scenarios.ts` presses `titleBar.sidebar.show` during narrow initialization.
- Verified: the console reports that no control matches the Simplified Chinese label for that message.
- Inferred: the narrow initialization step may no longer match the application's sidebar controls.

## Acceptance criteria

- The embedded overview scene reaches its ready state at phone width in Simplified Chinese.
- Initialization produces no missing-control error and leaves the expected preview scene usable.
