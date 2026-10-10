# Sending browse requests from the UI

## Bound browse requests by one count for the whole window, not per component

The daemon refuses a connection's browse request past 16 outstanding with `limit_exceeded` (the "Browse budgets"
section of `apps/daemon/PROTOCOL.md`), and a window talks to it over one connection, so that bound is shared by every
browse request the window sends: every project browser mounted, the viewer's read, and any new kind of request (the
Git modes' change lists and diffs included). A request once sent stays outstanding until the daemon answers it —
`DaemonClient` in `packages/ui/src/daemon-client.ts` gives a sent request no timeout, and unmounting the component that
sent it cancels nothing; only a newer request in the same `slot`, or the connection closing, ends it in the daemon. So
a component that is gone, or stuck behind a volume that stopped answering, still holds its requests against the bound.

Keep the concurrency limit for a kind of browse request in a count shared across the window at module level, as the
listings' `MAX_WINDOW_IN_FLIGHT` in `packages/ui/src/browser/useDirectoryListings.ts` is, and size it together with the
limits already there, the viewer's slots included, so that their sum stays under the daemon's bound. A limit per
mounted component looks safe for one component and lets a few of them together run into `limit_exceeded`.

A slot costs more than the one request in it: the daemon takes the outstanding unit for a request that claims a slot
before it cancels the request holding it, and the cancelled one keeps its own unit until it stops. So the sum reserves
a spare unit for each slot that can be replaced, not one for the window, since two slots can be replaced at the same
moment — and adding a slot lowers what the other kinds of request may have out at once.
