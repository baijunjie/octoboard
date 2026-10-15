# Reading daemon state in the UI

## An effect that must fire on one change depends on resolved values, never on a store map

Every daemon event that touches a single record replaces the whole map it lives in: the reducers in
`packages/ui/src/store.ts` build a `new Map` for `sessions`, `projects`, `consoles`, `pages` and `gitStatuses`. So a
map read through `useDaemonStore` has a new identity after every event of its kind, and those events arrive
constantly — every status change of every session anywhere is a `session_upserted`, and the git-status refresh
repeats on its own.

So a `useEffect` or `useMemo` that is to react to one particular change takes as its dependencies the resolved values
its body reads — the ids, the statuses, the record a predicate reaches by walking a chain of owners — and never the
map it read them out of, however much of the map it reads. A map among the dependencies makes it run again on every
unrelated event of that kind, and nothing flags it: the only symptom is whatever the extra runs do, which is why a
one-off check (is the selected session still part of this view?) ends up undoing a state the user set by some other
route.

A `useMemo` (or `useCallback`) goes one step further: its body reads only what its dependencies name, so resolve the
values into local variables above it and read those inside, as `currentConsoleId` is in `packages/ui/src/App.tsx`. A
memo whose body reads the map, or `x.y` where its dependencies say `x?.y`, makes the React Compiler skip the whole
component ("Existing memoization could not be preserved"), with no lint finding. A `useEffect` of that shape compiles
and may keep reading the map in its body.

The exception is a derivation whose job is to re-run on every event of that kind — the menu bar's session list, the
waiting notifications, a rail badge's activity — which does take the map itself.

## An empty collection means "nothing has arrived yet" until `hosts` is set

The store starts with `consoles`, `projects` and `sessions` empty and `hosts` undefined, and only the first `snapshot`
fills them (`State` in `packages/ui/src/store.ts`). `App` registers its hooks above the `if (!hosts)` early return, so
every hook does run through those pre-snapshot renders, on every launch. A predicate that reads like a fact about the
daemon — `consoles.size === 0`, no project under this host, this session is gone — is therefore true at the start of
*every* launch, and an effect keyed on its true-to-false edge fires on every launch that has any records at all, not
only in the case it was written for.

So gate anything that stands for "the daemon has none of these" on `hosts !== undefined`, and never on a collection
being empty alone; the hooks that need it take it as the `ready` flag `App` already passes them. A gallery scenario
cannot catch a missing gate: fixture scenarios are built with `hosts` already populated
(`packages/ui/src/gallery/fixtures/builders.ts`), so the pre-snapshot renders never happen there. Either keep the gate
visible at the call site in `App.tsx`, or drive the pre-snapshot inputs directly in a unit test of the hook.

## A request's reply arrives after its own change is already in the store

A mutating request's handler in `apps/daemon/src/coordinator.rs` broadcasts the change first and answers the caller
second, and those are two separate messages on the one control socket. `DaemonClient` applies each event to the store as
it arrives and resolves `request` only on the correlated reply, so by the time the code after an `await request(...)`
runs, the store already holds the change — and React may already have rendered on it, which is what decides whether the
component that sent the request, or anything the new state hides, is still mounted. That interleaving is not fixed, and
nothing fails loudly when it goes the other way.

So write what follows an `await request(...)` to be correct under either order: do not sample "has it happened yet?"
once, and do not assume the sender, or the surface the change replaces, is still there to act on. When something has to
happen exactly once after such a change, hold it as a debt — a ref set when the change is observed and cleared only once
the work is actually done — so whichever side lands first, the other finishes the job.

## A session's terminal title is not steady text: key nothing disruptive off it as sent

Agent CLIs animate a glyph at the start of their terminal title while they work (Claude Code cycles ◐◓◑◒ and shows ✳
when idle; Codex and other terminal tools use braille spinners), and each frame reaches the UI as a new
`terminal_title` on the session. So a working session's title changes continually even when nothing the user cares
about has changed. Anything that treats a title change as a real change — rebuilding a native menu or other surface
that resets when replaced, firing a notification, deduplicating by the title text, sending it on over IPC — churns at
the spinner's rate. Where a derived value must stay put while the session works, strip the leading glyphs first with
`withoutSpinnerGlyphs` (`packages/ui/src/lifecycle/statusItemMenu.ts`) rather than writing another pattern, and keep
showing the title as sent wherever the product docs say it is shown as the agent sent it.
