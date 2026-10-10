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

The exception is a derivation whose job is to re-run on every event of that kind — the menu bar's session list, the
waiting notifications, a rail badge's activity — which does take the map itself.
