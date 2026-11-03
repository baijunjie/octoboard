# Sending browse requests from the UI

## Bound browse requests by one count for the whole window, not per component

The daemon refuses a connection's browse request past 16 outstanding with `limit_exceeded` (the "Browse budgets"
section of `apps/daemon/PROTOCOL.md`), and a window talks to it over one connection, so that bound is shared by every
browse request the window sends: every project browser mounted, the viewer's read, and any new kind of request (the
Git modes' change lists and diffs included). A request once sent stays outstanding until the daemon answers it —
`DaemonClient` in `packages/ui/src/daemon-client.ts` gives a sent request no timeout, and unmounting the component that
sent it cancels nothing; only a newer request in the same `slot`, or the connection closing, ends it in the daemon. So
a component that is gone, or stuck behind a volume that stopped answering, still holds its requests against the bound.

So send every browse request through one of the window-wide lanes or slots of `packages/ui/src/browse/budget.ts`
(the "Shared entry points" section of `docs/structure/ui.md`), and give a new kind of request a share there, sized with
the others, rather than a concurrency limit of its own per mounted component: that looks safe for one component and
lets a few of them together run into `limit_exceeded`.
