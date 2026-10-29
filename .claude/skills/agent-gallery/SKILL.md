---
name: agent-gallery
description: Entry point for the UI state gallery — keep the gallery in step after a change to UI states, mocks or the interface contract mocks depend on, run the quick check on what was written to it, start a preview from this task's branch for the user to confirm at delivery, and clean it up after confirmation. Bring it to mind when a UI state is added or changed, or a UI change is to be handed to the user for confirmation. Use when the user says "add a gallery scenario", "show this state in the gallery", "the gallery is out of date", "clean up the gallery", "start a preview for me".
---

# UI state gallery

What follows sets only principles and the boundaries not to be crossed; judge the remaining details yourself.
When unsure, report back: stop, spell out what needs deciding (with the options) or what information is missing,
together with what you have already found out, and let the recipient of the report (the user, or the dispatcher
passing it on to the user) decide before going on; do not decide for yourself.

The gallery is the dev-only collection of scenarios: each shows one UI state over mock data, needing no real backend,
for people to confirm and for the quick check to walk one by one.

## Project conventions

"Project conventions" is filled in only by the installer and is not changed at run time. If an item you need is still
`<…>`, a command or path filled in has itself gone stale (unrelated to this change; missing environment such as a
database or network is reported as usual and does not count), or an item has to follow the project, stop and ask the
user to rerun the installer that installed this skill, saying which item must change and why.

A change that leaves them unable to run is treated as a failed quick check. With several UI packages the table has a
row each, and the items below the table say so where packages differ.

| Scope | Gallery approach | Where scenarios live | Opening the gallery | Scenario direct entry | Quick check (full) | Quick check (selected scenarios) |
|---|---|---|---|---|---|---|
| `packages/ui` | Self-built: `gallery.html` / `gallery-frame.html` over `packages/ui/src/gallery/` (the real `App` over the fixture daemon) | `packages/ui/src/gallery/fixtures/` — the area's file, or a new file listed in its `index.ts`; how a scenario is built and the conventions for writing one are in `packages/ui/README.md`, section "The UI state gallery". All of them are `gallery-writer`'s. The fixture-backed website preview (`packages/ui/src/website-preview/`) is not part of the gallery | `pnpm gallery --port <free port>` in `packages/ui` (the script's port 5175 is strict, so another checkout serving it fails the start; pick a free one; `BROWSER=none` in front keeps it from opening a browser tab); the address is the `Local:` line Vite prints | `http://localhost:<port>/gallery.html?scenario=<id>&theme=<light\|dark>&lang=<en\|zh-Hans\|…>`, plus `width=<px>`, `height=<px>`, `held=1` (shortcut modifier held) as needed; the frame alone, sized by the browser window, is `gallery-frame.html?scenario=<id>&theme=…&lang=…`, with `held=1` and `chrome=1` (the desktop window chrome) as needed | `pnpm gallery:sweep` in `packages/ui` (starts its own dev server on a free port) | `pnpm gallery:sweep <id or pattern …>`, or `--group <title>`; `--list` lists the ids |

- UI check items: render smoke (each scenario reaches `data-gallery-ready`, no `[data-gallery-failure]` overlay, no
  uncaught error): self-built `packages/ui/scripts/gallery-sweep.mjs` on Playwright; light; `en` only, `--lang <list>`
  or `--lang all` (every language with a catalog) when the user asks for other languages. It loads exactly the
  scenarios it is given, never skipping one; its page loads wait for a heavy-step slot (the lock `pnpm verify` also
  takes). The flags are in `packages/ui/README.md`, section "Sweeping the gallery".
  Narrow widths: the quick check also runs its scenarios with `--width 420,800`, as the page's width control would
  load them, since steps must run at any width the control picks.
  Kept out of the quick check and run only when the user asks: the dark theme, `pnpm gallery:sweep --theme dark`;
  chip geometry, self-built on Playwright, `node scripts/gallery-chip-geometry.mjs http://localhost:<port>` in
  `packages/ui` against a gallery served from this checkout (unlike the sweep it falls back to no other browser:
  without Playwright's own Chromium, set `OCTOBOARD_CHROMIUM` to one).
- What many scenarios share: `packages/ui/src/gallery/fixtures/` helpers and `packages/ui/src/gallery/fixtureDaemon.ts`
  (mocks shared by many scenarios), and the gallery machinery itself under `packages/ui/src/gallery/`, the gallery's
  pages and its scripts in `packages/ui/scripts/`.
- App preview: gallery only.
- Preview scratch location: `packages/ui/src/gallery/preview/` (ignored by git, so per checkout). Each `.ts` / `.tsx`
  file directly in it default-exports a `Scenario[]` that `packages/ui/src/gallery/fixtures/index.ts` appends to the
  catalog, so it must also pass the gallery's tests; it imports fixtures from their own modules, never from that
  `index.ts` (a cycle). Temporary mocks and helpers go in `preview/mocks/`, which the loader does not read. The record
  of services the preview started goes in `preview/services.md`.

## Changing the gallery

- Dispatch the `gallery-writer` subagent for every write to formal scenarios, mocks and the gallery machinery; with a
  subagent mechanism, write none of it yourself (without one, see the last item): dispatch it for a new or changed UI
  state, or a change to the interface contracts and types mocks depend on (even if the UI did not change); how a
  scenario is shown, which to extend and which to delete, is its judgement.
- One dispatch at a time, to keep each from adding its own; it cannot see your conversation, so each time hand over
  every UI state to be shown, what each is for a person to confirm, what changed in the contract, the files this
  change touches and the trade-offs the user settled, plus the address of any preview this working copy is serving.
- For what it reports as unshowable, needing app code changes, or trade-offs it could not settle: decide the minor
  ones yourself and leave the rest to the user; where the conventions must follow, handle it as "Project conventions"
  says.
- The subagent this skill dispatches is installed with it: with a subagent mechanism but no definition of the one to
  dispatch, the install is incomplete: stop and ask the user to rerun the installer that installed this skill. Without
  a subagent mechanism, read the installed definition and do the work by its rules yourself.

## Quick check

- The quick check guards the gallery, not the app (the app's correctness is for type checking and the tests); it runs
  only in the two places below, and when the user asks:
  - what `gallery-writer` wrote into the gallery, which it runs itself before reporting, with the range set by the next
    item;
  - the scenarios to be previewed for the user: sweep them by id ("Quick check (selected scenarios)") before giving the
    entries; those that passed when `gallery-writer` ran them are not rerun unless this change has changed since.
- For what was written into the gallery, the quick check runs only the scenarios added or changed; when something many
  scenarios use was changed, run the full check, since which scenarios it reaches cannot be told from the ones changed
  (the conventions' "What many scenarios share" lists only the known part and is not the limit).
- The quick check runs only in the source language (the language the UI copy was originally written in) and the default
  theme; a scenario that fixes a language or theme runs under its own condition. Whether language-dependent things are
  right (translations complete, formats correct, a scenario's steps finding their controls in other languages)
  belongs to type checking and the tests, and what can be written as a script check does not rely on rendering several
  languages. Sweeping other languages and themes happens only when the user asks, and for a subagent that means what
  whoever dispatched it passed on.
- Every scenario that runs must pass.
- Pass only by making things right: never delete scenarios, skip or loosen checks to make it pass, and never change a
  scenario or mock to accommodate an app error; updating a screenshot baseline after a deliberate scenario change is
  not loosening.

- Handle a failure by its cause:

  | Cause of failure | What to do |
  |---|---|
  | This change broke the app | Change the app code |
  | A scenario, mock or the gallery machinery is broken or behind | Dispatch `gallery-writer` to fix or delete it |
  | The command runs, and some scenario fails the same way on the baseline before the change (e.g. the branch to merge back into) | Do not fix, do not block delivery; state in the final reply that it failed before the change and does not count as passing |

  Verify the baseline in a separate copy, isolated as the conventions say and removed after use, leaving this working
  copy alone. Once fixed, whoever fixed it reruns the scenarios that failed, by id, until they pass.

- Passing the check only shows the scenarios run, not that they show the latest UI: where appearance and interaction
  need confirming, look at them for real.
- State in the final reply whether the full check, which scenarios, or none were run, and the result.

## Previewing for the user to confirm

- When this change touches the UI, give the user a preview: where whether it is right takes a human eye, wait for
  confirmation before delivering; where the user has already given the result and nothing needs judging (such as
  reordering, or changing one piece of copy), give the entry only and do not wait.
- Start the gallery from this task's branch or working copy, with the app preview where needed, avoiding other working
  copies as the conventions say; never borrow a service running elsewhere, which does not show this branch's changes.
- Give each view to confirm a direct entry: written as the convention's "Scenario direct entry", with the viewing
  conditions, using the actual address after start; open it yourself first to confirm it shows this branch's changes;
  one you cannot open yourself (such as a build on a real device) is given anyway, saying you could not confirm it.
- If no usable preview can be given, say plainly where it is stuck, and do not hand the UI change over as ready to
  confirm.
- Keep the preview running while waiting for confirmation; refresh it after acting on revision requests, and give new
  entries where they changed.
- Scenarios and mocks added only for the preview go into the "Preview scratch location", never mixed into the formal
  scenarios; when asking for confirmation, ask whether to keep them, and where you are not waiting for confirmation,
  still wait for the answer to this question; those to keep are handed to `gallery-writer` at once to be made formal
  scenarios, which it checks itself.
- Record the services you start and the builds you install in the "Preview scratch location", enough to recognise at
  cleanup that they are this working copy's.
- Never overwrite, stop or uninstall a service or build not started or installed by this working copy; when unsure,
  treat it as not yours and report: overwriting or uninstalling a build with the same app identifier deletes its data
  too.

## Cleanup

- Clean up before the branch is handed over (merge, push for a PR and so on), when the user abandons the change, or
  before the working copy is deleted.
- Stop the services and uninstall the builds the "Preview scratch location" records, and delete the temporary
  scenarios, mocks and records there; touch nothing outside it.
- Leave alone what you cannot verify (such as a device that cannot be reached), reporting where it is and how to
  recognise it; tell the user what was cleared.
- If the preview is not yet cleaned up at delivery, list in the final reply the services still running and the builds
  left, how to recognise them and how to clear them, so a working copy deleted directly does not leave them
  unrecognisable.
