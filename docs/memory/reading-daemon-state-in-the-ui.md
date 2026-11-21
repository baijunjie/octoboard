# Reading daemon state in the UI

The rules are in `docs/conventions/ui-code.md`, "Reading daemon state". What they leave out:

## Prove a component stayed idle by counting its executions, not by its DOM

Unchanged DOM does not show a component did not run again; count its executions, as `App.renderCounts.test.tsx` does.

## When `App.renderCounts.test.tsx` breaks after a change that only moved memoization, look for an unguarded set first

React drops a set to the value a state already holds only while the component has no update pending; otherwise the set
queues an update and the component runs once more, often later, when something in another lane renders it. So the extra
run of an unguarded set lands wherever that next render falls, and a change that only moved memoization can be what
exposes it.

## Code merged in that reads the whole `sessions` map in `App` or a list's parent is rewritten, never given the map back

A branch written before `App` stopped subscribing to the map may still read it there, or count on `App` or a list's
parent (the sidebar's `ConsoleBody`) rendering on every session change. When it is merged, rewrite it to select the
value it waits for (`archivingStatus` in `packages/ui/src/app/useArchiveSession.ts`) or to watch the DOM.
