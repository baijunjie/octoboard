# Writing UI code under the React Compiler

## Anything a render reads besides props, state, context and hook results is frozen at its first value

Every component and hook in `packages/ui` is compiled by the React Compiler, which caches each value on the inputs
it can see. A value read during render from anywhere else is not such an input, so the compiled code keeps the first
result until some visible input happens to change: the clock (`Date.now()`, including a helper that reads it by
default, as `formatRelativeTime` does when given no `now`), `store.getState()`, a module-level `let` or `Map`, an
object mutated in place. Before the compiler, every daemon event re-rendered the whole tree and hid this; no lint
rule flags it, and the stale value shows only once the app has sat still for a while (a "2 minutes ago" that never
moves, a selection that does not follow).

So feed a render only from props, state, context and hooks: pass `useNow()` (`packages/ui/src/useNow.ts`) as the
`now` of anything that formats a time, read the daemon store through a `useDaemonStore` selector, and move state that
changes in place behind a store or `useSyncExternalStore`. A value only a menu's items need (a row's menu judging
quotas against the clock) is not worth a `useNow()` that re-renders every row: give `ActionMenu` its `items` as a
function, which it calls when the menu opens and mounts its content, so that function may read `Date.now()` and
`store.getState()` itself. Where module state has to stay as it is, put
`"use no memo"` at the top of the component or hook, with a comment saying what it reads.

## A lint-clean component can still be left uncompiled: check the compiler's own log

When the compiler cannot compile a component or hook it skips the whole function, which then re-renders with every
parent render as before, and for most causes the lint reports nothing. In this codebase these were the causes:

- A ref written or read during render (the one cause the lint does flag, as `react-hooks/refs`). Keep the latest value
  with `useLatest`, or a handler with `useStableHandler`, both in `packages/ui/src/components/useLatest.ts`; the value
  is set in a layout effect, so a reader during render or in a descendant's layout effect in the same commit still
  sees the previous one.
- A `try` with a `finally`, or with no `catch`, anywhere in the function, nested callbacks included. Move the helper
  that holds it to module level.
- `??=`: write the assignment out.
- A `useMemo` or `useCallback` whose body reads more than its dependency list names ("Existing memoization could not be
  preserved"): resolve the values into locals above it and read only those inside.
- An `eslint-disable` comment for `react-hooks/exhaustive-deps` or `react-hooks/rules-of-hooks` anywhere in the
  function. Never add one; `exhaustive-deps` is off for this reason (`eslint.config.mjs`).

To see what did not compile, give the compiler a `logger` — temporarily as
`reactCompilerPreset({ logger: { logEvent(filename, event) { … } } })` in `packages/ui/vite.config.ts`, then build —
and look for `CompileError` events in the files you changed; a few functions elsewhere are left uncompiled on purpose,
each with a comment saying so.

## Whatever every row of a list takes must keep its identity across daemon events

Each daemon event replaces a store map, and with many sessions the events arrive constantly (every status change is
one), so the compiler's memoization is what keeps an event from re-rendering every row of the sidebar and the rail.
One prop that changes identity on every render re-renders every row again, with nothing to show for it but slowness:

- A handler that captures a store map, or anything else that changes per event, is wrapped in `useStableHandler`
  before it is passed to rows.
- An object handed to every row is built from named members, never gathered with a rest pattern (`...rest`), which
  is a new object on every render.
- A per-group array of sessions comes from the store's session index (`indexSessions` in
  `packages/ui/src/sessions/sessionIndex.ts`, read through `packages/ui/src/sessions/useSessions.ts`), whose arrays
  keep their identity until a session in them changes; any other array filtered out of a store map is kept
  identity-stable while its records are the same, through `useShallow`.
- Hooks come before the values derived from their inputs: the compiler keeps no value whose computation spans a hook
  call.
