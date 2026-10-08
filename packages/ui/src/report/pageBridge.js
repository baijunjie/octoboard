// Runs inside a report page's sandboxed frame, after the two scripts that define DOMPurify and the
// page (see `composePageDocument`); the page's own scripts never run. A page is a declarative
// document: this script sanitizes the page's HTML, renders the result, and relays the frame's
// native form submissions and a few keys to the window. It reads `DOMPurify` and `OCTOBOARD_PAGE`,
// which the scripts before it define.
(function () {
  // Everything the script uses is captured before the page's markup is inserted: a page can name an
  // element after a global (`<form name="FormData">`, `<img id="parent">`) and the document's
  // named-element lookup would then answer for it.
  var config = window.OCTOBOARD_PAGE;
  var sanitize = DOMPurify.sanitize.bind(DOMPurify);
  var post = parent.postMessage.bind(parent);
  var FormDataConstructor = FormData;
  var toArray = Array.from;
  var doc = document;
  var win = window;
  var forEach = Array.prototype.forEach;
  var querySelectorAll = Document.prototype.querySelectorAll;
  var getAttribute = Element.prototype.getAttribute;
  var setAttribute = Element.prototype.setAttribute;

  // What a page may never contain. DOMPurify drops all of it already; this list is both handed to
  // it as `FORBID_TAGS` and checked against its result, so a version of it that stopped doing so
  // renders nothing instead of the page. Media elements are forbidden too but not checked: they load
  // through fetches, which the frame's `default-src 'none'` already refuses.
  var NETWORK_TAGS = [
    "link",
    "iframe",
    "frame",
    "object",
    "embed",
    "meta",
    "base",
    "portal",
    "fencedframe",
  ];
  var INERT_TAGS = ["script", "noscript", "template"];

  // `RETURN_DOM` hands back the sanitized document's `<html>` element with `WHOLE_DOCUMENT`.
  // `SANITIZE_DOM` is off because it drops ordinary form field names (`name`, `id` colliding with a
  // document property), which a form needs; named-element clobbering is handled by the captures
  // above instead. The URI pattern admits no scheme at all, only values without one: it runs against
  // every attribute value DOMPurify has no other rule for, not only URLs, so a stricter one would
  // also strip `type`, `lang` and `dir`. A value opening with two slashes (either way, as the URL
  // parser reads a backslash as a slash) is a scheme-relative URL to another host, so it goes.
  // `data:` stays out of it: DOMPurify admits it on its own for an image's `src`, while a `data:`
  // link would navigate the frame to a document this frame's policy does not cover.
  var clean = sanitize(config.html, {
    WHOLE_DOCUMENT: true,
    RETURN_DOM: true,
    SANITIZE_DOM: false,
    FORBID_TAGS: NETWORK_TAGS.concat(INERT_TAGS, ["audio", "video", "source", "track"]),
    FORBID_ATTR: [
      "action",
      "formaction",
      "method",
      "enctype",
      "target",
      "ping",
      "srcset",
      "sizes",
      "nonce",
    ],
    ALLOWED_URI_REGEXP: /^(?:(?![\/\\]{2})[^a-z]|[a-z+.\-]+(?:[^a-z+.\-:]|$))/i,
  });
  var leftover = clean.querySelector(NETWORK_TAGS.concat(INERT_TAGS).join(", "));
  if (leftover) {
    // Nothing is rendered rather than the page as it came.
    return;
  }

  doc.addEventListener(
    "submit",
    function (event) {
      // The form never navigates the frame; its fields go to the window instead.
      event.preventDefault();
      if (config.history) return;
      var entries = [];
      // `event.submitter` is what puts a pressed `<button name value>` among the fields.
      toArray(new FormDataConstructor(event.target, event.submitter)).forEach(function (entry) {
        entries.push([entry[0], typeof entry[1] === "string" ? entry[1] : entry[1].name]);
      });
      post({ source: config.sources.submit, entries: entries }, "*");
    },
    true,
  );

  // Escape keeps its default action in the frame (closing an open `<dialog>` or `<select>`), while
  // F6 is the window's own key and the browser's default for it (moving focus to its own chrome) is
  // cancelled. ⌘[ and ⌘] (nothing else held) are the window's Back and Forward, relayed only where
  // the window has them (`config.historyKeys`) and cancelled in the frame. Ctrl+Tab, with or without
  // Shift and nothing else held, is the window's move between console sessions
  // (`config.switchKeys`): cancelled in the frame wherever the window has that shortcut, even when
  // it does nothing at the moment, since Ctrl+Tab has no use in a static page; a held key is
  // cancelled but relayed once. All are skipped while an IME composition is active, where Escape
  // cancels the composition rather than meaning "close". Present on history pages too.
  win.addEventListener(
    "keydown",
    function (event) {
      if (event.key === "Escape" && !event.isComposing && event.keyCode !== 229) {
        post({ source: config.sources.escape }, "*");
      }
      if (
        event.key === "F6" &&
        !event.isComposing &&
        !event.ctrlKey &&
        !event.altKey &&
        !event.metaKey
      ) {
        event.preventDefault();
        post({ source: config.sources.region, backward: event.shiftKey }, "*");
      }
      if (
        config.historyKeys &&
        (event.code === "BracketLeft" || event.code === "BracketRight") &&
        !event.isComposing &&
        event.metaKey &&
        !event.ctrlKey &&
        !event.altKey &&
        !event.shiftKey
      ) {
        event.preventDefault();
        post({ source: config.sources.history, backward: event.code === "BracketLeft" }, "*");
      }
      if (
        config.switchKeys &&
        event.key === "Tab" &&
        !event.isComposing &&
        event.ctrlKey &&
        !event.altKey &&
        !event.metaKey
      ) {
        event.preventDefault();
        if (!event.repeat) post({ source: config.sources.switch, backward: event.shiftKey }, "*");
      }
    },
    true,
  );

  // The desktop app shows no webview menu of its own, text fields aside; this frame is a separate
  // document, so the window's own rule does not reach it. `config.noContextMenu` is set only there.
  if (config.noContextMenu) {
    var closest = Element.prototype.closest;
    win.addEventListener(
      "contextmenu",
      function (event) {
        var target = event.target;
        if (
          target instanceof Element &&
          closest.call(target, 'input, textarea, [contenteditable]:not([contenteditable="false"])')
        ) {
          return;
        }
        event.preventDefault();
      },
      true,
    );
  }

  function render() {
    var head = doc.head;
    var root = doc.documentElement;
    var oldBody = doc.body;
    var cleanHead = clean.querySelector("head");
    var cleanBody = clean.querySelector("body");
    ["lang", "dir", "class", "style"].forEach(function (name) {
      var value = getAttribute.call(clean, name);
      if (value !== null) setAttribute.call(root, name, value);
    });
    while (cleanHead && cleanHead.firstChild) {
      head.appendChild(doc.adoptNode(cleanHead.firstChild));
    }
    root.replaceChild(doc.adoptNode(cleanBody), oldBody);
    // A history page is read-only: it cannot be filled in, and the `submit` handler above drops any
    // submission that gets through regardless.
    if (config.history) {
      forEach.call(querySelectorAll.call(doc, "input, select, textarea, button"), function (el) {
        el.disabled = true;
      });
    }
  }
  // The frame's own `<head>` and `<body>` have to exist before the page replaces them.
  if (doc.readyState === "loading") doc.addEventListener("DOMContentLoaded", render);
  else render();
})();
