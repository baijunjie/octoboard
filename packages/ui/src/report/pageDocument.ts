/** The `source` tag on the message the bridge script posts to the parent for an
 * `octoboard.submit()` call. Together with `ESCAPE_MESSAGE_SOURCE` it lets the listener in
 * `ReportPanel` tell the bridge's two message types apart from each other and from any other
 * `message` event arriving at the window (there is no origin to check instead — see `PageFrame`). */
export const SUBMIT_MESSAGE_SOURCE = "octoboard-page-submit";

/** The `source` tag on the message the bridge posts when Escape is pressed inside the page. It
 * carries nothing else: a keydown inside the sandboxed frame never reaches this window, so
 * without the relay Escape could not close a floating or drawer report panel while the page holds
 * focus. */
export const ESCAPE_MESSAGE_SOURCE = "octoboard-page-escape";

/** The `source` tag on the message the bridge posts when F6 or Shift+F6 is pressed inside the
 * page, for the same reason as `ESCAPE_MESSAGE_SOURCE`: without the relay focus could not leave
 * the frame by keyboard. It carries only `backward`, whether Shift was held. */
export const REGION_MESSAGE_SOURCE = "octoboard-page-region";

/** Restricts the page to an inlined, self-contained document with nowhere to phone out to for
 * its own content: `default-src 'none'` as the base, with only inline styles/script and `data:`
 * images allowed back in. `connect-src` and `form-action` are named explicitly even though
 * `default-src 'none'` already covers them, because what has to be denied is the point of this
 * policy, not an implementation detail of it.
 *
 * The frame also enforces the window's own policy from `index.html` on top of this one — that is
 * the layer self-navigation is blocked at, and where the `'unsafe-inline'` and `data:` sources
 * this layer relies on must also be allowed.
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

/** Keeps the page's default form controls and canvas rendering in their light variant regardless of
 * the OS preference. The panel deliberately stays a light "paper" surface in both app themes — a
 * report page is arbitrary model-authored HTML, and a page that hard-codes dark text would become
 * unreadable if it also inherited a dark native widget style — but without this, a browser honours
 * `prefers-color-scheme` for a document's own unstyled controls independently of any colour the
 * page's CSS sets, which `<iframe sandbox>` does not stop. */
const COLOR_SCHEME_META = '<meta name="color-scheme" content="light">';

/**
 * Builds the iframe's `srcdoc` document: the CSP meta tag, then the colour-scheme meta, then the
 * history lock (when applicable), then the bridge, then the page's own HTML, in that order — a CSP
 * delivered by `<meta>` only covers what the parser reaches after it. Composed as a plain string
 * and handed to React's `srcDoc` prop (a DOM property assignment, not string-concatenation into
 * surrounding markup), so nothing the page's HTML contains has an attribute-quote boundary to break
 * out of.
 */
export function composePageDocument(html: string, isHistory: boolean): string {
  const submitBody = isHistory
    ? `console.error("octoboard.submit() is disabled: this is a history page, read-only.");
       throw new Error("octoboard.submit() is disabled on a history page.");`
    : `parent.postMessage({ source: ${JSON.stringify(SUBMIT_MESSAGE_SOURCE)}, data: data }, "*");`;
  // Capture phase on the window, so the page's own handlers cannot swallow the key first; an
  // Escape is left alone so the page still sees it, while F6 is the window's own key and the
  // browser's default for it (moving focus to its own chrome) is cancelled. Skipped while an IME
  // composition is active, where Escape cancels the composition rather than meaning "close".
  // Present on history pages too, which stay unable to submit.
  const bridge = `<script>
    window.octoboard = {
      submit: function (data) {
        ${submitBody}
      },
    };
    window.addEventListener("keydown", function (event) {
      if (event.key === "Escape" && !event.isComposing && event.keyCode !== 229) {
        parent.postMessage({ source: ${JSON.stringify(ESCAPE_MESSAGE_SOURCE)} }, "*");
      }
      if (event.key === "F6" && !event.isComposing && !event.ctrlKey && !event.altKey && !event.metaKey) {
        event.preventDefault();
        parent.postMessage({ source: ${JSON.stringify(REGION_MESSAGE_SOURCE)}, backward: event.shiftKey }, "*");
      }
    }, true);
  </script>`;
  const historyLock = isHistory ? HISTORY_LOCK : "";
  return `<meta http-equiv="Content-Security-Policy" content="${PAGE_CSP}">${COLOR_SCHEME_META}${historyLock}${bridge}${html}`;
}
