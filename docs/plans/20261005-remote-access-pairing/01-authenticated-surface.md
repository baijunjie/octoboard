# 01 Authenticated surface

> Goal: the daemon has a separate external surface where nothing works without authentication, and the agent-facing
> endpoints are unreachable from outside the machine.
> Completion criteria: with the external surface on, an unauthenticated request or a socket from a foreign origin is
> refused on every route, and nothing on it is served over plain `http`; the hook and MCP endpoints do not answer on the
> external surface; with it off, nothing listens beyond loopback.

## Technical design

- [ ] The router and listener are split: internal (loopback; hook and MCP) and external (everything a client uses).
- [ ] TLS on the external surface.
- [ ] Authentication on every external route, including static files and WebSocket upgrades.
- [ ] Origin check on WebSocket upgrades.
- [ ] The setting that turns the external surface on, off by default in the desktop form; in the server form the
      surface is on from the start, there being no other.

## Implementation plan

- [ ] Split the surfaces first with no behavior change, then add the authentication layer to the external one.

## Handoff

- Before the external surface can be reached at a network address, the raised-hand notification paragraph of
  `docs/product/sessions.md` and the "A browser, blocked" row of the Notifications table in `docs/product/settings.md`
  have to be rewritten. Both assume every browser page is a secure context, which holds today only because the daemon
  refuses any `Host` but a loopback one. A page reached at a network address over plain `http` is not one, and Chrome
  still exposes `window.Notification` on such an origin: the platform adapter's `typeof Notification` guard therefore
  reports the capability present and the rail's bell appears, while the permission can never be granted and no site
  setting turns it on — so the row's "allowed again from the browser's own settings for the site" becomes wrong
  guidance rather than incomplete guidance. The docs have to say what a browser outside loopback gets, with this
  milestone's TLS as the reason the served page is a secure context at all.

## Notes for the developer

- **Reusable capabilities**: the MCP endpoint's per-launch token, which refuses an unknown token without detail.
- **Development notes**: default-deny; refusals carry no detail. A reverse proxy is no shortcut to this milestone: the
  access guard requires `Host` to be a loopback name and, on a request that carries one, `Origin` to be an application
  origin, a loopback origin or the same as `Host` — and a browser sends `Origin` on every WebSocket upgrade — so a proxy
  would have to rewrite both headers, and the daemon behind it would have nothing of its own to tell clients apart. The
  UI builds its WebSocket origin from the page's own: `wss:` from an `https:` page, `ws:` otherwise.
- **Reference docs**: `apps/daemon/PROTOCOL.md`, `docs/architecture.md`.
