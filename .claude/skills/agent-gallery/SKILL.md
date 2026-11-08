---
name: agent-gallery
description: Entry point for the UI state gallery — keep the gallery in step after a change to UI states, mocks or the interface contract mocks depend on, handle a failed quick check, start a preview from this task's branch for the user to confirm at delivery, and clean it up after confirmation. Bring it to mind when a UI state is added or changed, or a UI change is to be handed to the user for confirmation. Use when the user says "add a gallery scenario", "show this state in the gallery", "the gallery is out of date", "clean up the gallery", "start a preview for me".
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
| `packages/ui` | Self-built: `gallery.html` / `gallery-frame.html` over `packages/ui/src/gallery/` (the real `App` over the fixture daemon) | `packages/ui/src/gallery/fixtures/` — the area's file, or a new file listed in its `index.ts`; how a scenario is built and the conventions for writing one are in `packages/ui/README.md`, section "The UI state gallery". All of them are `gallery-writer`'s, and so is `packages/ui/src/fixtures/`, the shared fixtures runtime (fixture daemon, builders, sample data) that both the gallery and the website preview use. The fixture-backed website preview itself (`packages/ui/src/website-preview/`) is not part of the gallery | `pnpm gallery --port <free port>` in `packages/ui` (the script's port 5175 is strict, so another checkout serving it fails the start; pick a free one; `BROWSER=none` in front keeps it from opening a browser tab); the address is the `Local:` line Vite prints | `http://localhost:<port>/gallery.html?scenario=<id>&theme=<light\|dark>&lang=<en\|zh-Hans\|…>`, plus `width=<px>`, `height=<px>`, `held=1` (shortcut modifier held), `markdown=<tailwind\|github\|newsprint\|latex>` (Markdown style, viewer scenarios) as needed; the frame alone, sized by the browser window, is `gallery-frame.html?scenario=<id>&theme=…&lang=…`, with `held=1`, `markdown=…` and `chrome=1` (the desktop window chrome) as needed | `pnpm gallery:sweep` in `packages/ui` (starts its own dev server on a free port) | `pnpm gallery:sweep <id or pattern …>`, or `--group <title>`; `--list` lists the ids |

- UI check items: render smoke (each scenario reaches `data-gallery-ready`, no `[data-gallery-failure]` overlay, no
  uncaught error): self-built `packages/ui/scripts/gallery-sweep.mjs` on Playwright; light; `en` only, `--lang <list>`
  or `--lang all` (every language with a catalog) when the user asks for other languages. It loads exactly the
  scenarios it is given, never skipping one; its page loads wait for a heavy-step slot (the lock `pnpm verify` also
  takes). The flags are in `packages/ui/README.md`, section "Sweeping the gallery".
  Kept out of the quick check and run only when the user asks: the dark theme, `pnpm gallery:sweep --theme dark`;
  narrow widths, `pnpm gallery:sweep --width 420,800` (as the page's width control would load the scenarios, since
  steps must run at any width the control picks);
  chip geometry, self-built on Playwright, `node scripts/gallery-chip-geometry.mjs http://localhost:<port>` in
  `packages/ui` against a gallery served from this checkout (unlike the sweep it falls back to no other browser:
  without Playwright's own Chromium, set `OCTOBOARD_CHROMIUM` to one).
- What many scenarios share: `packages/ui/src/gallery/fixtures/` helpers, the shared fixtures runtime in
  `packages/ui/src/fixtures/` (its `fixtureDaemon.ts` mocks the daemon for every scenario; the website preview uses it
  too), and the gallery machinery itself under `packages/ui/src/gallery/`, the gallery's pages and its scripts in
  `packages/ui/scripts/`.
- App preview: gallery only.
- Preview scratch location: `packages/ui/src/gallery/preview/` (ignored by git, so per checkout). Each `.ts` / `.tsx`
  file directly in it default-exports a `Scenario[]` that `packages/ui/src/gallery/fixtures/index.ts` appends to the
  catalog, so it must also pass the gallery's tests; it imports fixtures from their own modules, never from that
  `index.ts` (a cycle). Temporary mocks and helpers go in `preview/mocks/`, which the loader does not read. The record
  of services the preview started goes in `preview/services.md`.
- Scenarios verify the UI, not each piece of copy. The same UI with different copy does not get another scenario, nor
  does a different value of a form control (select, switch, radio, segmented control, text field): one scenario shows
  the UI, and where the other copy or values are worth asserting, a test does it. A value or a piece of copy is a
  scenario of its own only where it changes the layout, such as the longest option label that must still fit. Error
  scenarios are merged only where the message has the same format and layout and differs only in values or wording;
  where the copy's format or layout differs (one line against several, a details block, a list, a long unbroken path,
  a title against none), or the actions offered differ (a different set of buttons), they stay separate scenarios. In
  this project this narrows `.claude/agents/gallery-writer.md`'s "Scenario standard": under "The number of mocks does
  not decide the number of scenarios", targets that differ only in copy or a control's value are not split even where
  showing one replaces the other, and being able to switch between them does not make them separate views; and
  "Viewing conditions do not get their own scenarios" extends beyond the gallery's own viewing controls to copy and
  form-control values. (Source: user instruction)
- Viewing conditions: in this project this narrows `.claude/agents/gallery-writer.md`'s "Scenario standard", under
  "Viewing conditions do not get their own scenarios". A layout that differs under a condition is what the gallery's
  viewing controls are for: the scenario that shows the state points at the condition in its description. Steps are
  written to run under whatever condition the controls pick, so no copy is made just to reach a control under it. A
  condition is fixed in the scenario only where the confirmation target cannot be reached by choosing that condition
  with the controls on another scenario, with the title or description saying why.
- Quick check scope and handling: this sets what the quick check covers and how a failure is handled; when it runs is
  set by `CLAUDE.md`'s "This project", not here. In this project this narrows "Quick check failure" below and
  `.claude/agents/gallery-writer.md`'s "Finishing" where they differ. (Source: user instruction)
  - What it guards: the gallery, not the app, whose correctness is for type checking and the tests.
  - What it covers: the scenarios `gallery-writer` wrote into the gallery and the scenarios previewed for the user, by
    id ("Quick check (selected scenarios)"); scenarios that already passed are not rerun unless this change has
    changed since. Of what was written into the gallery, only the scenarios added or changed, and the full check when
    something many scenarios use was changed, since which scenarios it reaches cannot be told from the ones changed
    ("What many scenarios share" lists only the known part and is not the limit).
  - Conditions: only the source language (`en`) and the light theme; a scenario that fixes a language or theme runs
    under its own condition. Whether language-dependent things are right (translations complete, formats correct, a
    scenario's steps finding their controls in other languages) belongs to type checking and the tests, and what can
    be written as a script check does not rely on rendering several languages. Other languages and themes are swept
    only when the user asks, and for a subagent that means what whoever dispatched it passed on.
  - Passing and baseline failures: every scenario that runs must pass. A failure that also occurs on the baseline
    before the change (e.g. the branch to merge back into) is not fixed and does not block delivery; state in the final
    reply that it failed before the change and does not count as passing. Verify the baseline in a separate copy,
    isolated from other checkouts as the "Opening the gallery" column says and removed after use, leaving this working
    copy alone.
  - Rerun and final reply: once a failure is fixed, whoever fixed it reruns the scenarios that failed, by id, until
    they pass. State in the final reply whether the full check, which scenarios, or none were run, and the result.
- `gallery-writer` also runs the project's type check and the relevant tests, and before starting runs the checks it
  will run at the end on the range it is about to change, noting the failures that already exist. A failure its change
  caused it fixes within what it owns; one that cannot be removed, that predates its change, or that is the app's
  behaviour being wrong it does not fix: it keeps its change and reports the failure with whose it is, marked uncertain
  where it cannot tell. In this project this narrows `.claude/agents/gallery-writer.md`'s "Finishing". (Source: user
  instruction)

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

## Quick check failure

- Pass only by making things right: never delete scenarios, skip or loosen checks to make it pass, and never change a
  scenario or mock to accommodate an app error.
- Every failure gets fixed, by cause: where the app code is wrong, change the app code; where a scenario, mock or the
  gallery machinery is broken or behind, dispatch `gallery-writer` to fix it, and delete a scenario only where its UI
  state no longer exists in the app.
- Passing the check only shows the scenarios run, not that they show the latest UI: where appearance and interaction
  need confirming, look at them for real.

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
  scenarios.
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
