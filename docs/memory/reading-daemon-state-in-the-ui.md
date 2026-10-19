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

A derivation whose job is to re-run on every event of that kind does not take the map through a render either: the
menu bar's session list subscribes to the store itself (`store.subscribe` in `useStatusItemMenu`) and rebuilds from
`getState()`, and the waiting notifications select the ids of the waiting sessions, an array that keeps its identity
until a session starts or stops waiting (`useShallow`).

## Read a session through the narrowest selector that answers the question

`App` does not subscribe to the `sessions` map: that would render the whole window on every status event of every
session. It reads the one session it needs (`useSession`), counts or derives a value inside a selector (the waiting
count, the aside owner, the archive's list), and calls `store.getState().sessions` inside a handler or effect that acts
on all of them. A list component reads its own group from the session index (`useProjectSessions`,
`useConsoleSessions`) and a row reads what it alone depends on (`useHasBoundSessions`, `useSession(bound_to)`). A
selector that builds a new object each call is wrapped in `useShallow`, or in `useDaemonStoreEqual` when the result
holds objects of its own. The daemon client hands the events that arrive within one frame to the store as one batch, and
the reducer keeps the record it holds when an upsert repeats it, so an event that changes nothing renders nothing.

So nothing may count on `App`, or on a list's parent such as the sidebar's `ConsoleBody`, rendering again when some
session changes. Code that worked only because they used to — an effect with no dependency list that checks a ref
against the store on every render, a hook that measures its children on each of its owner's renders — silently stops
running, and nothing fails. Select the value such code waits for, so that its change renders the component (as
`archivingStatus` in `packages/ui/src/App.tsx` does), or have it watch the DOM itself (`useFlip`'s
`ResizeObserver`). Code brought in from a branch that still reads the whole `sessions` map in such a component is
rewritten the same way when it is merged, never given the map back.

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

## A request's reply and the broadcast of its change reach the UI in either order

A mutating request's handler in `apps/daemon/src/coordinator.rs` broadcasts the change before it answers, but that
order is lost on the way: on the control socket the reply is queued by the request's own task and the broadcast by the
connection's `forward_broadcasts` task (`apps/daemon/src/server.rs`), two tasks feeding one outbound queue, so either
can reach the client first. `DaemonClient` then queues broadcasts and hands them to the store as one batch on the next
animation frame (after 250 ms at the latest, where frames are not running), while a reply first flushes the
broadcasts already queued and then settles the request. So when the code after an `await request(...)` runs, the
store may or may not hold the change yet, and React may or may not have rendered on it — which decides whether the
component that sent the request, or anything the new state hides, is still mounted. Nothing fails loudly when the
order flips.

So write what follows an `await request(...)` to be correct under either order:

- Where it needs the record the request created, seed the store from the reply only when the store lacks that id, and
  never overwrite one it holds: a held record came from a broadcast at least as new as the reply. `seedShellTerminal`
  in `packages/ui/src/store.ts` and the `session_opened` case of its reducer have that shape; a new kind of record
  gets a seed like them.
- Do not sample "has it happened yet?" once, and do not assume the sender, or the surface the change replaces, is
  still there to act on. When something has to happen exactly once after such a change, hold it as a debt — a ref set
  when the change is observed and cleared only once the work is actually done — so whichever side lands first, the
  other finishes the job.

## Tell which of two events came first with a counter, never a clock

Applies to `packages/ui` wherever one event must be known to have come after another: was this request sent after the
session switch, is this reply newer than the one on screen. Do not compare `performance.now()` or `Date.now()`
readings for it. The app's WebKit rounds `performance.now()` to whole milliseconds (as Playwright's WebKit shows), so
two readings in one handler, or in handlers that run close together, are usually equal: a strict `>` then never holds
and a `>=` cannot tell the two orders apart. jsdom's clock is sub-millisecond, so the component tests pass either way.

Stamp such events from a counter that grows on every read instead — the Git mode's `gitMoment` in
`packages/ui/src/browser/gitRequests.ts`, or one of the same shape outside it — and keep clocks for durations and
wall-clock time only (refresh intervals, relative times, expiry).

## A session's terminal title arrives without its spinner glyph

Agent CLIs animate a glyph at the start of their terminal title while they work (Claude Code cycles ◐◓◑◒ and shows ✳
when idle; Codex and other terminal tools use braille spinners). The daemon strips those leading glyphs before it
stores or publishes the title (`apps/daemon/src/terminal_title.rs`), so a working session does not send a
`terminal_title` update per frame and no UI code needs to strip them again: the title in the store is the steady text,
and deduplicating by it, rebuilding a native menu from it or notifying on it does not churn at the spinner's rate.
Do not add a glyph pattern to the UI; if one slips through, fix the daemon's list.
