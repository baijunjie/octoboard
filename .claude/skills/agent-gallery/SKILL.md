---
name: agent-gallery
description: Entry point for the UI state gallery — keep the gallery in step after a change to UI states, mocks or the interface contract mocks depend on, run the quick check to confirm every scenario is sound, start a preview from the development branch for the user to confirm at delivery, and clean it up after confirmation. Bring it to mind when a UI state is added or changed, or a UI change is to be handed to the user for confirmation. Use when the user says "add a gallery scenario", "show this state in the gallery", "the gallery is out of date", "clean up the gallery", "start a preview for me".
---

# UI state gallery

What follows sets only principles and the boundaries not to be crossed; judge the remaining details yourself, and
report back when unsure: stop, state what needs deciding and what has been established, and wait for the decision
before going on.

The gallery is the dev-only collection of scenarios: each shows one UI state over mock data, needing no real backend,
for people to confirm and for the quick check to walk one by one.

## Project conventions

This section is filled in by the installer from the project's state and the user's choices, rechecked against the
project's state on reinstall, with what cannot be settled carried over; with several UI packages the table has a row
each, and the items below the table say so where packages differ.

| Scope | Gallery approach | Where scenarios live | Opening the gallery | Scenario direct entry | Quick check (full) | Quick check (selected scenarios) |
|---|---|---|---|---|---|---|
| `packages/ui` | Self-built: `gallery.html` / `gallery-frame.html` over `packages/ui/src/gallery/` (the real `App` over the fixture daemon) | `packages/ui/src/gallery/fixtures/` — the area's file, or a new file listed in its `index.ts`; how a scenario is built and the conventions for writing one are in `packages/ui/README.md`, section "The UI state gallery". All of them are `gallery-writer`'s. The fixture-backed website preview (`packages/ui/src/website-preview/`) is not part of the gallery | `pnpm gallery --port <free port>` in `packages/ui` (the script's port 5175 is strict, so another checkout serving it fails the start; pick a free one; `BROWSER=none` in front keeps it from opening a browser tab); the address is the `Local:` line Vite prints | `http://localhost:<port>/gallery.html?scenario=<id>&theme=<light\|dark>&lang=<en\|zh-Hans\|…>`, plus `width=<px>`, `height=<px>`, `held=1` (shortcut modifier held) as needed; the frame alone, sized by the browser window, is `gallery-frame.html?scenario=<id>&theme=…&lang=…`, with `held=1` and `chrome=1` (the desktop window chrome) as needed | `pnpm gallery:sweep` in `packages/ui` (starts its own dev server on a free port) | `pnpm gallery:sweep <id or pattern …>`, or `--group <title>`; `--list` lists the ids |

- UI check items: render smoke (each scenario reaches `data-gallery-ready`, no `[data-gallery-failure]` overlay, no
  uncaught error): self-built `packages/ui/scripts/gallery-sweep.mjs` on Playwright; `en`, `zh-Hans`; light. Kept out
  of the quick check: the dark theme, `pnpm gallery:sweep --theme dark`, run when a change touches colours; chip
  geometry, self-built on Playwright, `node scripts/gallery-chip-geometry.mjs http://localhost:<port>` in `packages/ui`
  against a gallery served from this checkout (unlike the sweep it falls back to no other browser: without
  Playwright's own Chromium, set `OCTOBOARD_CHROMIUM` to one), run when a change touches inline chips.
- Known failure: at the default parallelism a full sweep can report failures marked "only under load" (a page crash, a
  hover step such as `terminal-link-tooltip` opening no tooltip) that a serial run does not. Where they are in
  scenarios this change does not touch, rerun just those with `pnpm gallery:sweep --jobs 3 <ids …>`; passing there
  counts as passing.
- App preview: gallery only.
- Preview scratch location: `packages/ui/src/gallery/preview/` (ignored by git, so per checkout). Each `.ts` / `.tsx`
  file directly in it default-exports a `Scenario[]` that `packages/ui/src/gallery/fixtures/index.ts` appends to the
  catalog, so it must also pass the gallery's tests; it imports fixtures from their own modules, never from that
  `index.ts` (a cycle). Temporary mocks and helpers go in `preview/mocks/`, which the loader does not read. The record
  of services the preview started goes in `preview/services.md`.

## Changing the gallery

1. **Dispatch the `gallery-writer` subagent for every write to formal scenarios, mocks and the gallery machinery; write
   none of it yourself**: how a scenario is shown, which to extend and which to delete, is its judgement. The "Preview
   scratch location" is not its; see "Previewing for the user to confirm".
2. **One dispatch at a time, handing over every state to be shown**: it has to judge across all scenarios which to
   extend and which to delete, and several at once would each add their own. It cannot see your conversation: hand over
   the new or changed UI states, what each is for a person to confirm, and the trade-offs the user settled. Dispatch it
   again afterwards to fix check failures, after app code changed, or when the user asks for changes; where this
   working copy is serving a preview, include its address.
3. Act on its report: for states reported as unreachable, or showable only by changing app code, judge whether to
   change it, and leave a large change to the user; leave trade-offs it could not settle to the user; for scenarios
   whose address it reports changed or broken, give the user new entries for any already sent; where it reports that a
   command or check item in the conventions must follow, update "Project conventions" accordingly.
4. Without a subagent mechanism, write it yourself to the standard in the `gallery-writer` subagent definition installed
   in the project.

## Quick check

- When this change touches the UI, mocks, or the interface contracts and types mocks depend on, run the **full** quick
  check before delivery; every scenario must pass: changes to shared components, styles and mocks reach scenarios that
  were not touched, which running only the touched ones does not find.
- Diagnose a failure first: broken by this change, fix the code; a scenario or mock that has fallen behind the current
  UI or contract, dispatch `gallery-writer` to fix it, leaving failed scenarios not its own to the user; failing the
  same way on the development branch's base and unrelated to this change — if the conventions record it as a known
  failure, handle it as recorded without asking again, otherwise leave it to the user, and if they decide to leave it
  for now, state in the final reply that it is a known failure and does not count as passing. Never make it pass by
  deleting scenarios, skipping or loosening checks.
- Passing only shows the scenarios run, not that they show the latest UI: where appearance and interaction need
  confirming, look at them in a browser, simulator or device.
- State in the final reply what was run and the result.

## Previewing for the user to confirm

When this change touches the UI and is to be handed to the user to confirm:

1. **Start the preview from the development branch**: start the gallery in this task's branch or worktree, with the app
   preview where needed, avoiding other working copies as the conventions say; never borrow a service running in
   another working copy, which does not show this branch's changes. Record the services started and the builds put on
   a simulator or device in the "Preview scratch location"; a recorded one still running and showing this branch is
   reused as it is, and the record of one no longer there is deleted.
2. **Give direct entries**: in the report give each view to confirm a direct entry, written as the convention's
   "Scenario direct entry" with the viewing conditions, using the actual address after start; open it yourself first to
   confirm it shows this branch's changes, then attach any access instructions needed.
3. **Keep it running while waiting for confirmation**: act on the user's revision requests, refresh the preview after
   each, and give new entries where they changed.
4. If no usable preview can be given, say plainly where it is stuck, and do not hand the UI change over as ready to
   confirm.

Scenarios or mocks added only to show the user, not meant to stay in the gallery, are written by you into the "Preview
scratch location", never mixed into the formal scenarios; when there are such scenarios, ask the user while requesting
confirmation whether to keep them; those to keep are handed to `gallery-writer` as UI states to show as formal
scenarios, and then the temporary copy is deleted.

## Cleanup

After the user confirms, or agrees to hand the branch over (merge, push for a PR and so on), and before it is handed
over; and before the working copy is deleted, where it will be:

- Stop the services recorded in this working copy's "Preview scratch location" and remove the recorded preview builds;
  services and builds outside the record are left alone.
- Turn the temporary scenarios the user wants kept into formal ones first, then delete the temporary scenarios, mocks
  and records in this working copy's "Preview scratch location"; files outside it are not deleted.
- Tell the user the preview is stopped and what was cleared.

## When the conventions cannot be found

The "Project conventions" table is not filled in (only `<…>` left), or a command in it no longer works (the command,
script or path no longer exists and fails the same way on the development branch's base): stop, and ask the user to run
the `setup-agent:gallery` installer to settle the project conventions again. Where this change is what broke it, treat
it as a failed quick check.
