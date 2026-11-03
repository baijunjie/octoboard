# Reading daemon state in the UI

## An effect that must fire on one change depends on resolved values, never on a store map

Every daemon event that touches a single record replaces the whole map it lives in: the reducer in
`packages/ui/src/store/reducer.ts` builds a `new Map` for `sessions`, `projects`, `consoles`, `pages` and
`gitStatuses`. So a map read through `useDaemonStore` has a new identity after every event of its kind, and those
events arrive constantly — every status change of every session anywhere is a `session_upserted`, and the git-status
refresh repeats on its own.

So a `useEffect` or `useMemo` that is to react to one particular change takes as its dependencies the resolved values
its body reads — the ids, the statuses, the record a predicate reaches by walking a chain of owners — and never the
map it read them out of, however much of the map it reads. A map among the dependencies makes it run again on every
unrelated event of that kind, and nothing flags it: the only symptom is whatever the extra runs do, which is why a
one-off check (is the selected session still part of this view?) ends up undoing a state the user set by some other
route.

A `useMemo` (or `useCallback`) goes one step further: its body reads only what its dependencies name, so resolve the
values into local variables above it and read those inside. A memo whose body reads the map, or `x.y` where its
dependencies say `x?.y`, makes the React Compiler skip the whole component ("Existing memoization could not be
preserved"), with no lint finding. A `useEffect` of that shape compiles and may keep reading the map in its body.

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
`archivingStatus` in `packages/ui/src/app/useArchiveSession.ts` does), or have it watch the DOM itself (`useListMotion`'s
`ResizeObserver`). Code brought in from a branch that still reads the whole `sessions` map in such a component is
rewritten the same way when it is merged, never given the map back.

## A stored record is what the daemon sent; what only this window knows is applied where it is read

The store's daemon collections (`sessions`, `projects`, `consoles` and the rest in `packages/ui/src/store/state.ts`)
hold each record exactly as the last event sent it. State that only this window has, such as the sessions its own
account-switch batch has queued (`queuedAccountSwitches`), is a field of its own and is never written into those
records; a new window-only state of the kind takes the same shape. Whether an account switch holds a session goes
through the entry point `docs/structure/ui.md`, section "Shared entry points", names. The shown status, ranking and the
menu bar's list take the queue as their `queuedSwitches` argument, next to `delegating`, so a component that sorts or
shows sessions selects the queue itself (`useQueuedAccountSwitches`, or `s.queuedAccountSwitches` inside its
selector): otherwise it does not render again when a session joins or leaves the queue, and nothing fails.

## Deduplicate polling answers before scheduling state

For daemon polling whose hook runs in `App`, compare each answer with the latest committed state before calling
the setter: returning the previous state from a functional updater can still execute `App` once after a changed
update. Read that state through `useLatest`, never a ref assigned during render, and skip the setter for unchanged
answers, repeated refresh failures included. Compare the error's message and displayed diagnostics explicitly:
`sameValue` compares enumerable properties, which omit an Error's message and stack. A successful answer must still
clear a previous failure even when its list is unchanged. Count executions of the hook's caller to verify this;
unchanged DOM or a skipped child does not prove that `App` stayed idle.

## An empty collection means "nothing has arrived yet" until `hosts` is set

The store starts with `consoles`, `projects` and `sessions` empty and `hosts` undefined, and only the first `snapshot`
fills them (`State` in `packages/ui/src/store/state.ts`). `App` registers its hooks above the `if (!hosts)` early
return, so every hook does run through those pre-snapshot renders, on every launch. A predicate that reads like a fact about the
daemon — `consoles.size === 0`, no project under this host, this session is gone — is therefore true at the start of
*every* launch, and an effect keyed on its true-to-false edge fires on every launch that has any records at all, not
only in the case it was written for.

So gate anything that stands for "the daemon has none of these" on `hosts !== undefined`, and never on a collection
being empty alone; the hooks that need it take it as the `ready` flag `App` already passes them. A gallery scenario
cannot catch a missing gate: fixture scenarios are built with `hosts` already populated
(`packages/ui/src/fixtures/builders.ts`), so the pre-snapshot renders never happen there. Either keep the gate
visible at the call site in `App.tsx`, or drive the pre-snapshot inputs directly in a unit test of the hook.

## A request's reply and the broadcast of its change reach the UI in either order

A mutating request's handler under `apps/daemon/src/coordinator/` broadcasts the change before it answers, but that
order is lost on the way: on the control socket the reply is queued by the request's own task and the broadcast by the
connection's `forward_broadcasts` task (`apps/daemon/src/server/control.rs`), two tasks feeding one outbound queue, so
either can reach the client first. `DaemonClient` then queues broadcasts and hands them to the store as one batch on the
next animation frame (after 250 ms at the latest, where frames are not running), while a reply first flushes the
broadcasts already queued and then settles the request. So when the code after an `await request(...)` runs, the store
may or may not hold the change yet, and React may or may not have rendered on it — which decides whether the component
that sent the request, or anything the new state hides, is still mounted. Nothing fails loudly when the order flips.

So write what follows an `await request(...)` to be correct under either order:

- Where it needs the record the request created, seed the store from the reply only when the store lacks that id, and
  never overwrite one it holds: a held record came from a broadcast at least as new as the reply. `seedShellTerminal`
  in `packages/ui/src/store/actions.ts` and the `session_opened` case in `packages/ui/src/store/reducer.ts` have that
  shape; a new kind of record
  gets a seed like them.
- Do not sample "has it happened yet?" once, and do not assume the sender, or the surface the change replaces, is
  still there to act on. When something has to happen exactly once after such a change, hold it as a debt — a ref set
  when the change is observed and cleared only once the work is actually done — so whichever side lands first, the
  other finishes the job.
- A control that shows a value the daemon holds, such as a switch bound to a setting, shows the store's value. Do not
  show a local value that is set on press and cleared when the request settles. When the reply arrives before the
  broadcast, the control briefly shows the old value again, and then the new one. To stop a second press while the
  first is unanswered, ignore that press; use a ref, not a value that is shown.
- That does not hold for a control whose every edit sends the whole value it shows, such as a list of tags saved on
  each change: the user's next edit is built on what is shown, so showing the store's value while a save is
  unanswered drops what that save added, and such edits cannot be ignored. Keep a local value, the list last sent,
  shown while any save is outstanding (count them). Once none is, keep showing it until the store holds that list, or
  holds a list none of this run's saves explains — neither the stored list the first save started from nor any list
  sent — which is a concurrent writer's or a different normalisation, and is shown and built on from then on. Clearing
  the local value when the request settles shows the old list again; clearing it only when the store equals it hides
  a concurrent writer's change for good. When the last outstanding save fails, report it and show the store's value
  again. `packages/ui/src/settings/accounts/AgentTagsSetting.tsx` has that shape; a new control of the kind copies it.

## Tell which of two events came first with a counter, never a clock

Applies to `packages/ui` wherever one event must be known to have come after another: was this request sent after the
session switch, is this reply newer than the one on screen. Do not compare `performance.now()` or `Date.now()`
readings for it. The app's WebKit rounds `performance.now()` to whole milliseconds (as Playwright's WebKit shows), so
two readings in one handler, or in handlers that run close together, are usually equal: a strict `>` then never holds
and a `>=` cannot tell the two orders apart. jsdom's clock is sub-millisecond, so the component tests pass either way.

Stamp such events from a counter that grows on every read instead — the Git mode's `gitMoment` in
`packages/ui/src/project-browser/git/gitRequests.ts`, or one of the same shape outside it — and keep clocks for durations and
wall-clock time only (refresh intervals, relative times, expiry).

## A session's terminal title arrives without its spinner glyph

Agent CLIs animate a glyph at the start of their terminal title while they work (Claude Code cycles ◐◓◑◒ and shows ✳
when idle; Codex and other terminal tools use braille spinners). The daemon strips those leading glyphs before it
stores or publishes the title (`apps/daemon/src/terminal/title.rs`), so a working session does not send a
`terminal_title` update per frame and no UI code needs to strip them again: the title in the store is the steady text,
and deduplicating by it, rebuilding a native menu from it or notifying on it does not churn at the spinner's rate.
Do not add a glyph pattern to the UI; if one slips through, fix the daemon's list.
