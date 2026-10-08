import purify from "dompurify/dist/purify.min.js?raw";

import bridge from "./pageBridge.js?raw";

/** The `source` tag on the message the bridge script posts to the parent for a submitted form.
 * Together with `ESCAPE_MESSAGE_SOURCE` it lets the listener in `ReportPanel` tell the bridge's
 * message types apart from each other and from any other `message` event arriving at the window
 * (there is no origin to check instead — see `PageFrame`). */
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

/** What the page may do with the network, stated once: nothing. The only script the frame runs is
 * Octoboard's own, which `script-src` admits by the per-document `nonce`, so a script the page
 * carries (which the bridge removes first, in any case) has no source to run from. `connect-src`
 * and `form-action` are named explicitly even though `default-src 'none'` already covers them,
 * because what has to be denied is the point of this policy, not an implementation detail of it.
 *
 * The frame also enforces the window's own policy from `index.html` on top of this one — that is
 * the layer self-navigation is blocked at, and where the `'unsafe-inline'` and `data:` sources
 * this layer relies on must also be allowed.
 *
 * CSP does not govern `RTCPeerConnection` or `<link rel="preconnect">` / `rel="dns-prefetch"`, nor
 * does a parent's `frame-src` stop a nested `<iframe srcdoc>`; those are closed by the bridge
 * never rendering a script, a `<link>` or a frame at all. The nonce-only `script-src` is the second
 * layer for scripts, so a script the sanitizer missed still cannot reach WebRTC; for the links it is
 * the window's content rule list (`apps/desktop/src-tauri/src/page_isolation.rs`).
 *
 * A `<meta>`-delivered CSP cannot carry `frame-ancestors`, and nothing here needs it. */
function pageCsp(nonce: string): string {
  return [
    "default-src 'none'",
    `script-src 'nonce-${nonce}'`,
    "style-src 'unsafe-inline'",
    "img-src data:",
    "connect-src 'none'",
    "form-action 'none'",
    "frame-src 'none'",
    "base-uri 'none'",
  ].join("; ");
}

/** Keeps the page's default form controls and canvas rendering in their light variant regardless of
 * the OS preference. The panel deliberately stays a light "paper" surface in both app themes — a
 * report page is arbitrary model-authored HTML, and a page that hard-codes dark text would become
 * unreadable if it also inherited a dark native widget style — but without this, a browser honours
 * `prefers-color-scheme` for a document's own unstyled controls independently of any colour the
 * page's CSS sets, which `<iframe sandbox>` does not stop. */
const COLOR_SCHEME_META = '<meta name="color-scheme" content="light">';

/** DOMPurify's minified build, without its trailing source-map comment, which names a `.map` file
 * that does not exist in the frame. */
const PURIFY_SOURCE = purify.replace(/\n?\/\/# sourceMappingURL=.*$/, "");

/** JSON that is safe inside a `<script>`: a `<` can only occur inside a string there, and escaping
 * it as `\u003c` means nothing in `value` can close the element or open a comment. */
function scriptJson(value: unknown): string {
  return JSON.stringify(value).replace(/</g, "\\u003c");
}

/**
 * Builds the iframe's `srcdoc` document: the CSP meta tag, then the colour-scheme meta, then
 * Octoboard's own scripts, in that order — a CSP delivered by `<meta>` only covers what the parser
 * reaches after it. The page is never part of the document's markup: it travels as a string
 * literal to the bridge (`pageBridge.js`), which sanitizes it with DOMPurify and renders the result,
 * so a page is a static document with native forms and its own scripts never run. Composed as a
 * plain string and handed to React's `srcDoc` prop (a DOM property assignment, not
 * string-concatenation into surrounding markup).
 *
 * `nonce` has to be unguessable and fresh for each document: it is what tells Octoboard's scripts
 * from anything else the frame might come to hold.
 *
 * `noContextMenu` makes the frame suppress its own right-click menu, outside text fields, as the
 * desktop app does for the rest of its window.
 */
export function composePageDocument(
  html: string,
  isHistory: boolean,
  nonce: string,
  noContextMenu = false,
): string {
  const page = {
    html,
    history: isHistory,
    noContextMenu,
    sources: {
      submit: SUBMIT_MESSAGE_SOURCE,
      escape: ESCAPE_MESSAGE_SOURCE,
      region: REGION_MESSAGE_SOURCE,
    },
  };
  const config = `window.OCTOBOARD_PAGE = ${scriptJson(page)};`;
  return (
    `<meta http-equiv="Content-Security-Policy" content="${pageCsp(nonce)}">${COLOR_SCHEME_META}` +
    `<script nonce="${nonce}">${PURIFY_SOURCE}</script>` +
    `<script nonce="${nonce}">${config}</script>` +
    `<script nonce="${nonce}">${bridge}</script>`
  );
}

/** The fields a submitted form carries, as the object `submit_page` sends: field names as keys in
 * document order, a repeated name (a checkbox group) with its values joined by ", ". `undefined`
 * when `entries` is not the `[name, value]` string pairs the bridge posts, which only a forged
 * message can be. */
export function submissionFromEntries(entries: unknown): Record<string, string> | undefined {
  if (!Array.isArray(entries)) return undefined;
  // A `Map`, so a field named `__proto__` or `constructor` is just a key.
  const fields = new Map<string, string>();
  for (const entry of entries) {
    if (!Array.isArray(entry) || typeof entry[0] !== "string" || typeof entry[1] !== "string") {
      return undefined;
    }
    const earlier = fields.get(entry[0]);
    fields.set(entry[0], earlier === undefined ? entry[1] : `${earlier}, ${entry[1]}`);
  }
  return Object.fromEntries(fields);
}
