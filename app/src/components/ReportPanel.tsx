import React, { useCallback, useEffect, useRef, useState } from "react";

import type { Page } from "../protocol";
import { useDaemon } from "../store";

/** The `source` tag on the one message type the bridge script posts to the parent, so the
 * listener below can tell an `octoboard.submit()` call apart from any other `message` event
 * arriving at the window (there is no origin to check instead — see `PageFrame`). */
const SUBMIT_MESSAGE_SOURCE = "octoboard-page-submit";

/**
 * The console's report panel: shown only for the hub session, which is what makes a console's
 * pages visible at all (there is nowhere else to show them). Lists pages on mount and on every
 * reconnect — the daemon never replays a missed `page_created` on its own (see the `page_list`
 * row under "Daemon to client" in `daemon/PROTOCOL.md`), so re-listing is the only way to recover
 * from one. A console switch mounting a fresh instance is the call site's concern, not this
 * component's.
 */
export function ReportPanel({ consoleId }: { consoleId: string }): React.ReactElement {
  const { pages, connectionState, snapshotEpoch, request, toastError } = useDaemon();
  const consolePages = pages.get(consoleId);

  // The user's position, held as the id of the page they paged to rather than an index —
  // `undefined` means "following the newest". An id survives the list changing size or shape
  // (a reconnect's re-list, or a `page_created` appending) because it is looked up fresh on every
  // render instead of being carried across one as a stale index would be.
  const [anchorId, setAnchorId] = useState<string>();

  // Re-list on mount and on every snapshot. `snapshotEpoch` (rather than `connectionState`) is
  // what makes lag recovery re-list too: the daemon answers a lagged broadcast receiver with a
  // fresh `snapshot` on the same socket, so the connection never goes through a state change of
  // its own.
  useEffect(() => {
    if (connectionState !== "open") return;
    // Guards only this effect's own reaction to its request — not the global store, which is
    // keyed per console and so never has this console's and another's replies collide. What this
    // guards against is a slow reply for a console the user has since switched away from (and
    // possibly back to) landing a now-stale error after the fact.
    let stale = false;
    request({ type: "list_pages", console: consoleId }).catch((err) => {
      if (!stale) toastError((err as Error).message);
    });
    return () => {
      stale = true;
    };
  }, [consoleId, connectionState, snapshotEpoch, request, toastError]);

  const handleSubmit = useCallback(
    (page: Page, data: unknown) => {
      // The daemon refuses `submit_page` for any page that is not this console's newest (see
      // "Client to daemon" in `daemon/PROTOCOL.md`), so a stale submission in flight from a page
      // the user has since paged away from is caught there, not here — this just forwards it and
      // reports whatever comes back.
      request({ type: "submit_page", page: page.id, data }).catch((err) => {
        toastError((err as Error).message);
      });
    },
    [request, toastError],
  );

  // `undefined` means the first `list_pages` is still in flight. Render the panel's own empty box
  // rather than `null`: `null` drops the panel out of the flex row entirely, and the terminal pane
  // resizing to fill the gap and back is a real SIGWINCH to the agent on every hub switch and lag
  // recovery, not just a layout flicker.
  if (consolePages === undefined) return <div className="report-panel" />;

  if (consolePages.length === 0) {
    return <div className="report-panel report-panel-empty">No pages yet.</div>;
  }

  // Look the anchor up fresh rather than trusting a remembered index: an id either still names a
  // page in the current list, or it does not (never pushed past "not found" by a resize), and
  // falling back to the newest on a miss is exactly the "re-arm following" behavior `goTo` needs
  // when the user pages forward to it.
  const anchorIndex = anchorId !== undefined ? consolePages.findIndex((p) => p.id === anchorId) : -1;
  const displayIndex = anchorIndex === -1 ? consolePages.length - 1 : anchorIndex;
  const page = consolePages[displayIndex];
  const isHistory = displayIndex !== consolePages.length - 1;

  const goTo = (index: number) => {
    const target = consolePages[index];
    // Anchoring to the newest page's id would also work, but clearing the anchor instead makes
    // "following" its own state again: the next `page_created` needs no special-casing to keep
    // the view on the new newest page.
    setAnchorId(index === consolePages.length - 1 ? undefined : target.id);
  };

  return (
    <div className="report-panel">
      <div className="report-panel-bar">
        <button type="button" onClick={() => goTo(displayIndex - 1)} disabled={displayIndex === 0}>
          ◀
        </button>
        <span className="report-panel-position">
          {displayIndex + 1} / {consolePages.length}
        </span>
        <button
          type="button"
          onClick={() => goTo(displayIndex + 1)}
          disabled={displayIndex === consolePages.length - 1}
        >
          ▶
        </button>
        <span className="report-panel-time">{new Date(page.created_at).toLocaleString()}</span>
        {isHistory && <span className="report-panel-history-badge">Read-only</span>}
      </div>
      <PageFrame key={page.id} page={page} isHistory={isHistory} onSubmit={handleSubmit} />
    </div>
  );
}

/**
 * The sandboxed render of one page. Keyed by `page.id` at the call site so React remounts this
 * (and so the iframe) rather than mutating it across a page change — a page's script must not keep
 * running after the user pages away.
 */
function PageFrame({
  page,
  isHistory,
  onSubmit,
}: {
  page: Page;
  isHistory: boolean;
  onSubmit: (page: Page, data: unknown) => void;
}): React.ReactElement {
  const iframeRef = useRef<HTMLIFrameElement>(null);

  useEffect(() => {
    // A history page's bridge already throws instead of posting (see `composeSrcDoc`), but a
    // page's own script can reach the parent directly with `parent.postMessage(...)`, skipping
    // that throw. The daemon still refuses the resulting `submit_page` (it is the enforcement
    // point — see `handleSubmit` above), but forwarding it at all would surface that refusal as an
    // error toast for an action the user never took, so a history page's listener does not forward
    // in the first place.
    if (isHistory) return;
    const handleMessage = (event: MessageEvent) => {
      // `sandbox="allow-scripts"` without `allow-same-origin` gives the frame an opaque origin, so
      // its messages arrive with `event.origin === "null"` — a string every opaque frame shares,
      // not something that identifies this one. The only reliable check is that the message came
      // from this iframe's own window.
      if (event.source !== iframeRef.current?.contentWindow) return;
      const data = event.data as { source?: unknown; data?: unknown } | null;
      if (!data || data.source !== SUBMIT_MESSAGE_SOURCE) return;
      onSubmit(page, data.data);
    };
    window.addEventListener("message", handleMessage);
    return () => window.removeEventListener("message", handleMessage);
  }, [page, isHistory, onSubmit]);

  return (
    <iframe
      ref={iframeRef}
      className="report-page-frame"
      title="Report page"
      sandbox="allow-scripts"
      srcDoc={composeSrcDoc(page.html, isHistory)}
    />
  );
}

/** Restricts the page to an inlined, self-contained document with nowhere to phone out to for
 * its own content: `default-src 'none'` as the base, with only inline styles/script and `data:`
 * images allowed back in. `connect-src` and `form-action` are named explicitly even though
 * `default-src 'none'` already covers them, because what has to be denied is the point of this
 * policy, not an implementation detail of it. Verified live: this blocks every subresource load,
 * `fetch`/XHR, external `<script>`/`<link>`, and native form submission.
 *
 * The frame also enforces the window's own policy from `app/index.html` on top of this one —
 * that is the layer self-navigation is blocked at, and where the `'unsafe-inline'` and `data:`
 * sources this layer relies on must also be allowed.
 *
 * A `<meta>`-delivered CSP cannot carry `frame-ancestors`, and nothing here needs it. */
const PAGE_CSP = [
  "default-src 'none'",
  "script-src 'unsafe-inline'",
  "style-src 'unsafe-inline'",
  "img-src data:",
  "connect-src 'none'",
  "form-action 'none'",
  "frame-src 'none'",
  "base-uri 'none'",
].join("; ");

/** Disables every control once the document is parsed, so a history page cannot be filled in and
 * submitted even though the bridge's `submit` throws — the throw lands inside the sandboxed
 * frame, where nothing surfaces it to the user. The `<style>` is a backstop for a control the
 * page's own script adds after this script runs; CSS keeps matching new elements on its own,
 * where a one-time `querySelectorAll` pass would not. */
const HISTORY_LOCK = `<style>input, select, textarea, button { pointer-events: none !important; }</style>
<script>
  document.addEventListener("DOMContentLoaded", function () {
    document.querySelectorAll("input, select, textarea, button").forEach(function (el) {
      el.disabled = true;
    });
  });
</script>`;

/**
 * Builds the iframe's `srcdoc` document: the CSP meta tag, then the history lock (when
 * applicable), then the bridge, then the page's own HTML, in that order — a CSP delivered by
 * `<meta>` only covers what the parser reaches after it. Composed as a plain string and handed to
 * React's `srcDoc` prop (a DOM property assignment, not string-concatenation into surrounding
 * markup), so nothing the page's HTML contains has an attribute-quote boundary to break out of.
 */
function composeSrcDoc(html: string, isHistory: boolean): string {
  const submitBody = isHistory
    ? `console.error("octoboard.submit() is disabled: this is a history page, read-only.");
       throw new Error("octoboard.submit() is disabled on a history page.");`
    : `parent.postMessage({ source: ${JSON.stringify(SUBMIT_MESSAGE_SOURCE)}, data: data }, "*");`;
  const bridge = `<script>
    window.octoboard = {
      submit: function (data) {
        ${submitBody}
      },
    };
  </script>`;
  const historyLock = isHistory ? HISTORY_LOCK : "";
  return `<meta http-equiv="Content-Security-Policy" content="${PAGE_CSP}">${historyLock}${bridge}${html}`;
}
