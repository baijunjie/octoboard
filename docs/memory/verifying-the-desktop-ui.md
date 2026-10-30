# Verifying behaviour in the desktop app

## Keep a verification as narrow as the change

The user has objected to drawn-out testing. Verify in the app only the behaviour this change altered, in a short,
targeted run — the states, languages or screens the change actually touches, not a tour of the rest or a matrix of
every combination — and leave what a unit test or the type checker already covers to them. Where a question can be
settled by a program without a browser (whether a catalog is complete, whether a step would find its control in
another language), settle it there, extending a test where none covers it, rather than by rendering the gallery in more
languages or themes: the user prefers that check to a wider render.

A fallback written for an environment this machine does not have is settled by reading the code, not by finding that
environment: a browser too old for an event the code works around, a platform the build does not target. The user has
said as much — a compatibility detail of that kind wants its logic confirmed, not a device. What a real run is for is
what only the real thing can answer: the shell's own dialogs, a volume that is really unmounted, how long something
really takes, whether a window really paints.

Hunting a UI defect across the whole app (text landing on half pixels, a style that may be missing anywhere) is no
reason to render everything: find the candidates by reading the code first, then render or measure only those, one
scenario each, a few pages at a time. Rendering every gallery scenario in two languages with six browser pages at once
has eaten several GB of memory and left orphaned processes behind, and the user asked why it was not done by code
analysis. A brief that hands such work to a subagent names which scenarios, how many and the maximum number of pages
open at once — "walk the gallery's scenarios" is read as all of them — and requires every browser and server it
started to be stopped before it reports.

Changing a shared component's markup, layout or timing widens what the change touches to every surface already
using it, so those surfaces are inside this narrow scope rather than outside it: making `MarkedPath`'s root a `span`
for one new caller collapsed the path boxes in the Settings rows, and neither the type checker, the unit tests nor a
gallery pass over the new caller's own surface showed it. Settle such a change by putting each existing call site
side by side against the merge base — a second gallery server run from a branchless temporary worktree at that
commit (`git worktree add --detach <path> <commit>`) — rather than judging the new rendering on its own.

Where what the change alters is the data a fixture answers with rather than any markup, settle it on the replies instead
of on renderings: dump every scenario's reply for the affected requests to JSON at the merge base and on the branch and
diff the two dumps. That names which field of which reply moved, which no screenshot comparison can, and it costs one
scripted run rather than two servers and a capture pass. Where renderings do have to be compared, no scenario is
byte-identical between two runs, and how far from identical is a property of that scenario rather than of the capture
pass: an animation moves a few dozen pixels in one small region (the session-status glyph), while a floating HeroUI
overlay lands a couple of dozen CSS pixels away from one render to the next, and a lazily loaded renderer may not have
painted at all when the capture is taken — either of those moves tens of thousands of pixels, more than a real change to
the same scenario does. So measure that floor before reading any diff as a regression: capture the whole set twice at
the same commit, and count a scenario as changed only where its branch-against-base diff stands above its own
same-commit diff.

## Look at a UI state in the gallery before staging it through a daemon

To see how the UI renders in a given state — connection loss, a session status, a dialog, the archive, report pages,
toasts, a narrow window, a language or the right-to-left layout — open it in the UI state gallery
(`packages/ui/README.md`, section "The UI state gallery") rather than building the state up against a running daemon.
Where no scenario shows the state, do not write one yourself: hand it to the `agent-gallery` skill, which dispatches
the subagent that owns the gallery and the criteria for what a scenario is worth carrying (`CLAUDE.md`, section
"Dispatching subagents"). The gallery is the real `App` over a fixture daemon in the Vite dev server, so it settles
rendering, layout and copy and nothing more: it has no real daemon timing, protocol round trips or agent terminal, and
it is not the app's WKWebView. A pointer interaction or a WebKit rendering question is still checked with the gallery
page loaded in Playwright's `webkit`, a paint defect still in the real window, and behaviour that depends on the
daemon, the shell or how the window starts up still in the real app.

Whether to sweep the gallery (`pnpm gallery:sweep`) at all, and on which scenarios, languages and themes, is set by the
"Quick check" section of `.claude/skills/agent-gallery/SKILL.md`; a change touching the UI is not by itself a reason to
sweep. A page with no errors does not establish that its mocks supply what the current components need: verify the
fields, data conditions and contracts behind a new UI state against the code.

How the bundle loads what it fetches at run time — lazy chunks, a worker, WebAssembly, assets — and anything a
build-only Vite plugin changes are not settled there either: the gallery runs Vite's unbundled dev modules from
`http://localhost`, while the app loads the built `dist/` from `tauri://localhost`. Check those in a packaged build
(`pnpm build:app`). For UI the app does not render yet, build that from a temporary detached worktree
(`git worktree add --detach`) whose `main.tsx` renders the gallery's scenarios in place of the app.

The gallery's "File viewer" group hands `FileViewer` a fixed subject instead of driving the app, so it carries none of
what the project browser supplies while running — above all the loader for a change's whole bodies, which is the only
thing that makes the renderer offer to expand the lines a diff collapsed. A diff's expansion is therefore never
exercised there, however right the scenario looks: verify it in the "Project pane: Git" group, whose fixture change
supplies those bodies, and when handing a new change presentation to the `agent-gallery` skill, say that it has to be
shown in that group.

That group also stands in for the app around the viewer, in both directions, and only running its pages shows a
mismatch. The contexts the app would provide are wired for that branch alone in `packages/ui/src/gallery/frame.tsx`, so
a viewer component that starts calling a hook needing one — anything that reaches the daemon, a toast included — throws
on mount in every scenario of the group, with the type check and the unit tests still green. The other way round, its
subjects are literals standing in for what the project browser's own hooks build, so a field those hooks fill only
under a condition (a `ViewerSubject`'s `source`, which only the change reader sets, and only for a comparison) is left
unset in every fixture that does not meet that condition, read off the hook that builds the subject rather than off
what the header happens to draw. So open the group's pages for any change to the viewer or to what it depends on.

## Freeze tests as well as application source during final gallery verification

Finish edits to the gallery checkout before a final sweep or capture, including test files. The UI's Tailwind Vite
plugin scans source files independently of JavaScript imports, so a test-only edit can trigger a stylesheet hot
update in the running gallery. Record which source tree the result covers and keep that tree unchanged until the run
ends. If source changes during verification, retain the run as intermediate evidence and verify the final tree after
it is stable; do not certify it from a sweep that spanned edits.

## Only `zh-Hans` shows translated copy; the other 15 languages render English

Applies to any check whose subject is translated text: reading a screen under another language, a gallery scenario's
step that names a control, a test that asserts on what a surface says. Only `en` and `zh-Hans` have catalogs
(`CATALOGS` in `packages/ui/src/i18n/catalog.ts`; `docs/product/language.md`, section "What is translated so far"), so
each of the other 15 offered languages renders English word for word. Anything resting on English copy — a name written
out as English text, a label that never reached the catalog, a layout that only fits English — passes under every one of
them, so a sweep across languages is no broader than a run in English. Make such a check `zh-Hans`, and expect no
language to show text longer than English's, since none of the longer-worded ones is translated. Text direction is not a
catalog's (`textDirection` in `packages/ui/src/i18n/languages.ts`), so `ar` still settles mirroring.

## A fixture daemon answers as fully as the real one, or the gallery shows a state no daemon sends

Applies whenever a daemon reply gains a field the UI draws from, and whenever a gallery scenario still shows the old
behaviour after a fix. The fixture daemon (`packages/ui/src/gallery/fixtureDaemon.ts`, and `fixtureGit.ts` for a
project's Git mode) answers every scenario from what that scenario's fixture happens to hold, so a field the fixture
cannot supply goes out as the real daemon's "not known" value and the UI draws the fallback for it. A complete fix can
therefore leave every scenario showing what it showed before, with nothing in the type checker or the unit tests to say
which side is wrong: the viewer's bar for the lines past a diff's last hunk went on claiming more might follow because
no fixture change carried both sides' whole text for the new line count to be taken from. So before reading a scenario
as a fix that did not work, check that its fixture can produce the input the fix reads at all. Where a reply gains such
a field, the fixture has to be taught it as `packages/ui/README.md`, section "The UI state gallery", under
"Conventions for writing scenarios", sets out.

The public website's product preview is one more scenario over the same fixture daemon (`createPreviewScenario` in
`packages/ui/src/website-preview/scenarios.ts`), but it is not among the gallery's scenarios, so neither
`pnpm gallery:sweep` nor the guard test over the fixture daemon's replies
(`packages/ui/src/gallery/fixtures/projectGit.test.ts`) reads it, and the `agent-gallery` skill does not keep it in
step. When a change makes something the preview shows draw on daemon state it did not before, give that scenario the
state too and look at the preview itself (`pnpm --filter @octoboard/web dev`): left out, the feature renders empty
there and nothing fails.

## A final capture does not verify earlier transient states

Applies when visually verifying gallery interactions. A capture taken after setup shows only the view it leaves on
screen; it does not visually verify earlier states, even when their assertions passed. Display initialization is not
a regression sequence: keep necessary behavioral assertions in tests or separate verification runs.

## Settle what `git` writes against real `git` in a throwaway repository

Applies to `git` output written out by hand rather than taken from a run of `git`: a patch in a gallery change fixture
or a viewer fixture, an expected patch in a viewer test. Nothing here can see such a literal being wrong — the type
check sees a string, a unit test compares it against itself, and the gallery renders whatever it says — so it is worth
no more than the reading of whoever wrote it. Build the case in a throwaway repository, run the command the daemon runs
for it, and take the output from there.

Two premises had stood in these fixtures as literals for a long time, and both were wrong: that `git` writes zeros in a
patch's `index` line for a side it read from the worktree disk (it hashes the file; it is `git diff --raw` that writes
zeros for that side), and that a rename whose content is unchanged has an empty patch (`git` writes the rename's headers
with no hunk; only a change at one path whose two sides are equal is empty).

## Stage a defect's states in the order it was reported, not in whatever order is easy to reach

The order the states are brought up in decides the outcome on its own often enough — whether an element was already
mounted when an overlay opened, what held focus when a region appeared, which of two things observed the other — that
a run reaching the same final state by another route proves nothing about the route that was reported. A gallery
scenario here raised the connection banner and then opened a dialog over it, where the defect had been filed the other
way round; every measurement passed, and the fix it passed still failed in the packaged app. So when reproducing a
defect or verifying its fix, put the steps in the order the report gives, and where a scenario cannot reach the state
that way, extend it until it can rather than reporting the pass the other order gives.

## Check the page in WebKit with real pointer input and focus scrolling, and with long and CJK text

The app renders in WKWebView, so a pass in Chrome says nothing: a HeroUI tag's remove button that ignored the mouse in
WebKit passed a scripted click in Chrome. Run a browser check of the page in Playwright's `webkit` and drive the pointer
through `page.mouse` rather than a click dispatched from script. The same holds for where focus scrolls a scroller:
the engines' focus scrolls differ in timing and in what CSS they honour (see the "Focus scrolled to the edge of a
scroller that fades there" section of `docs/ui-accessibility.md`), so a `scroll-margin` fix that holds a list still in
Chrome can leave it jumping in WebKit. Playwright is a pinned devDependency of `packages/ui`,
so `pnpm install` is all such a check needs from the repository; the dependency downloads no browser of its own, and the
browser is fetched explicitly with `pnpm exec playwright install webkit`, run from `packages/ui` — from the workspace
root `pnpm exec playwright` resolves to a machine-global Playwright or to nothing, and would silently fetch the wrong
revision. Headless WebKit still is not the app for how things paint (it has no GPU compositing), so a rendering defect
is judged in the real window. Where a change puts text in a constrained layout, include a long text and a CJK one:
overflow defects there showed only with long text. Judge a colour or a dimming from computed styles or full-resolution
pixels, not from a downscaled screenshot. A packaged build offers no DOM to measure, so settle a geometry question there
from ink extents read off a capture at the display's native resolution, allowing for its scale factor (a Retina display
gives 2 image pixels per CSS pixel), and settle a question of overlap or reachability with a real click at the measured
point, since a capture only ever shows what is on top. Colours read off such a capture carry the display's own profile,
so a contrast ratio measured from one differs from the ratio the tokens give by a few tenths, either way: it is good for
confirming which side of a threshold a pair lands on, and never for correcting a token-derived ratio recorded in the
stylesheet or the accessibility standard.

A scrollbar's own footprint is one layout question that check cannot answer, however much else it settles: Playwright's
WebKit draws overlay scrollbars only — headless and headed, at `scrollbar-width` `auto` or `thin`, and even with macOS
set to show scroll bars always — so a scroller over content larger than its box on both axes reports
`offsetWidth - clientWidth` and `offsetHeight - clientHeight` of 0 there. Anything about something sitting under,
beside, or dimmed by a scrollbar that takes space therefore shows nothing wrong in that check, and has to be measured
in real Safari on the gallery page, or in the app. That wants the system set to show scroll bars always
(`defaults write -g AppleShowScrollBars -string Always`), which is honoured — but only by a browser started
afterwards: a Safari that was already running when the setting changed keeps drawing overlay scrollbars and reads as
no defect. Read the old value before writing it and put it back afterwards, it being the user's own machine-wide
setting.

## Settle a touch gesture through the browser's own touch input, not jsdom or dispatched events

A jsdom test and a gallery step dispatch touch or pointer events straight at an element (`ui.swipe` in
`packages/ui/src/gallery/interact.ts` sends plain `Event`s). Neither goes through the browser's gesture handling,
which is where a gesture breaks. Pointer events are the case in point. Once Chromium starts treating a sideways touch
as a pan, it sends `pointercancel` after the first `pointermove`, even on a page with nothing to scroll sideways, so a
pointer-event swipe never reaches its threshold while the tests pass. `touchmove` keeps coming, so detect a finger
gesture from touch events. Do not reach for `touch-action` to keep pointer events alive: it would also take away
vertical scrolling.

To verify a gesture, drive the gallery page in Playwright's Chromium with a context created with
`isMobile: true, hasTouch: true`. Send the touch over CDP: `Input.dispatchTouchEvent` with `touchStart`, several
`touchMove`s and `touchEnd`. Playwright's own `touchscreen` only taps. CDP exists only in Chromium, so this exercises
Chromium's pipeline, not WebKit's.

## Bisect every terminal symptom against the daemon before blaming the agent

When the app shows a terminal symptom — a keystroke doing nothing, input dying after a session switch, a pane looking
stale — send the same bytes straight to the daemon's terminal WebSocket and compare before concluding anything. On
screen a UI defect and ordinary agent behaviour are indistinguishable: "Ctrl+C is being swallowed above the terminal",
"the UI rebound the read path on a session switch but not the write path" and "this agent simply ignores Esc in this
state" all look exactly alike, and only the side-by-side comparison tells them apart. Do this before filing a symptom
against an agent or against `xterm.js`.

## Watch for a report page's blocked navigation on the window, not inside the frame

What stops a page in the panel from navigating its frame somewhere external is the *embedder's* `frame-src`, so WebKit
reports the refusal to the embedder: the `securitypolicyviolation` event fires on the window's own document and the
frame's document never sees it. Instrument the window — a listener there is both the only way to observe a page
attempting to leave and a standing health signal for the window's own policy.

## A network-level probe of this window needs its positive control from outside the app

While the window ships a restrictive CSP (`default-src 'none'`), every channel a probe would normally use as its
positive control — a `fetch`, an `<img>`, an `<iframe>`, a WebSocket — is refused before any name is resolved, so a
capture that comes back empty says nothing about whether the app can reach the network. Generate the control outside
the app: Safari's lookups leave through the same WebKit networking path.

## Prove that no key reached the terminal against an agent that echoes every byte

When a verification has to show that keys the UI handles itself — in a pane, a viewer, a dialog over the terminal —
never reach the selected session, run that session on a stand-in agent CLI that writes back everything it reads, not
on a real agent. A real agent shows nothing for many keys (an arrow, Escape or Enter at an empty prompt), so a key
that leaked looks the same as one that did not, and the check passes whatever the UI does; against the echo, any leak
shows up in the terminal.
