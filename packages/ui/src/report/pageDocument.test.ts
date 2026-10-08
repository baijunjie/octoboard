import { JSDOM } from "jsdom";
import { describe, expect, it } from "vitest";

import {
  composePageDocument,
  HISTORY_MESSAGE_SOURCE,
  type PageDocumentOptions,
  submissionFromEntries,
  SUBMIT_MESSAGE_SOURCE,
  SWITCH_MESSAGE_SOURCE,
} from "./pageDocument";

const NONCE = "0123456789abcdef";

/** Renders a page the way the sandboxed frame does: the composed document runs its own scripts in
 * a fresh window and the bridge builds the page's DOM. Returns that window's document, and the
 * messages the bridge posted to its parent (here, the window itself). */
async function render(html: string, isHistory = false, options: PageDocumentOptions = {}) {
  const dom = new JSDOM(composePageDocument(html, isHistory, NONCE, options), {
    runScripts: "dangerously",
  });
  const { window } = dom;
  const messages: unknown[] = [];
  window.addEventListener("message", (event) => messages.push(event.data));
  await new Promise((resolve) => window.addEventListener("load", resolve));
  return { document: window.document, messages, window };
}

describe("the report page's sanitizer", () => {
  it("removes every script and network channel, and keeps what a document needs", async () => {
    const { document } = await render(`<!doctype html><html lang="ja" dir="rtl"><head>
      <link rel="preconnect" href="http://127.0.0.1:1"><link rel="dns-prefetch" href="//x.example">
      <meta http-equiv="refresh" content="0;url=https://x.example"><base href="https://x.example/">
      <style>p { color: red }</style><script>window.ran = 1</script></head><body onload="window.ran = 2">
      <p style="color: blue" onclick="window.ran = 3">text</p>
      <a href="javascript:window.ran = 4">js</a><a id="ext" href="https://x.example/">ext</a>
      <a id="rel" href="//x.example/">rel</a><a id="rel2" href="\\\\x.example/">rel</a>
      <a id="data" href="data:text/html,x">data</a>
      <iframe srcdoc="<script>1</script>"></iframe><img id="img" src="data:image/png;base64,AAAA">
      <form action="https://x.example/" method="post"><input name="name" type="text" required>
      <button name="choice" value="a">A</button></form></body></html>`);
    expect(document.querySelector("script[src], iframe, base, [onclick], [onload]")).toBeNull();
    expect(document.querySelector("link, noscript, template")).toBeNull();
    // Only the CSP and colour-scheme metas Octoboard puts in the frame.
    expect(document.querySelectorAll("meta")).toHaveLength(2);
    // The only scripts left are the three Octoboard puts in the frame.
    expect(document.querySelectorAll("script")).toHaveLength(3);
    expect(document.querySelector("#ext")?.hasAttribute("href")).toBe(false);
    expect(document.querySelector("#rel")?.hasAttribute("href")).toBe(false);
    expect(document.querySelector("#rel2")?.hasAttribute("href")).toBe(false);
    expect(document.querySelector("#data")?.hasAttribute("href")).toBe(false);
    expect(document.querySelector('a[href^="javascript"]')).toBeNull();
    expect(document.querySelector("form")?.hasAttribute("action")).toBe(false);
    expect(document.querySelector("form")?.hasAttribute("method")).toBe(false);
    expect(document.querySelector('input[name="name"][type="text"][required]')).not.toBeNull();
    expect(document.querySelector('button[name="choice"][value="a"]')).not.toBeNull();
    expect(document.querySelector("style")?.textContent).toContain("color: red");
    expect(document.querySelector("p")?.getAttribute("style")).toBe("color: blue");
    expect(document.querySelector("#img")?.getAttribute("src")).toMatch(/^data:/);
    expect(document.documentElement.getAttribute("lang")).toBe("ja");
    expect(document.documentElement.getAttribute("dir")).toBe("rtl");
  });

  it("posts a submitted form's fields with the pressed button, and none from a history page", async () => {
    const page = `<form><input name="who" value="Ada"><button name="choice" value="yes">Yes</button></form>`;
    const live = await render(page);
    live.document.querySelector("button")!.click();
    await new Promise((resolve) => setTimeout(resolve));
    expect(live.messages).toEqual([
      {
        source: SUBMIT_MESSAGE_SOURCE,
        entries: [
          ["who", "Ada"],
          ["choice", "yes"],
        ],
      },
    ]);

    const history = await render(page, true);
    expect(history.document.querySelector("input")?.hasAttribute("disabled")).toBe(true);
    history.document
      .querySelector("form")!
      .dispatchEvent(new history.window.Event("submit", { bubbles: true, cancelable: true }));
    await new Promise((resolve) => setTimeout(resolve));
    expect(history.messages).toEqual([]);
  });
});

describe("the report page's key relay", () => {
  const press = (window: Awaited<ReturnType<typeof render>>["window"], init: KeyboardEventInit) => {
    const event = new window.KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
    window.document.body.dispatchEvent(event);
    return event;
  };

  it("relays ⌘[ and ⌘] where the window has them, and keeps them from the page", async () => {
    const { window, messages } = await render("<p>x</p>", false, { relayHistoryKeys: true });
    expect(press(window, { code: "BracketLeft", metaKey: true }).defaultPrevented).toBe(true);
    expect(press(window, { code: "BracketRight", metaKey: true }).defaultPrevented).toBe(true);
    await new Promise((resolve) => setTimeout(resolve));
    expect(messages).toEqual([
      { source: HISTORY_MESSAGE_SOURCE, backward: true },
      { source: HISTORY_MESSAGE_SOURCE, backward: false },
    ]);
  });

  it("relays nothing else: not the bracket keys with another modifier, not other keys, not elsewhere", async () => {
    const { window, messages } = await render("<p>x</p>", false, { relayHistoryKeys: true });
    for (const init of [
      { code: "BracketLeft" },
      { code: "BracketLeft", metaKey: true, shiftKey: true },
      { code: "BracketLeft", metaKey: true, altKey: true },
      { code: "BracketLeft", metaKey: true, ctrlKey: true },
      { code: "KeyA", metaKey: true },
    ]) {
      expect(press(window, init).defaultPrevented).toBe(false);
    }
    const elsewhere = await render("<p>x</p>");
    expect(press(elsewhere.window, { code: "BracketLeft", metaKey: true }).defaultPrevented).toBe(false);
    await new Promise((resolve) => setTimeout(resolve));
    expect(messages).toEqual([]);
    expect(elsewhere.messages).toEqual([]);
  });

  it("relays Ctrl+Tab and Ctrl+Shift+Tab where the window has them, and keeps them from the page", async () => {
    const { window, messages } = await render("<p>x</p>", false, { relaySwitchKeys: true });
    expect(press(window, { key: "Tab", ctrlKey: true }).defaultPrevented).toBe(true);
    expect(press(window, { key: "Tab", ctrlKey: true, shiftKey: true }).defaultPrevented).toBe(true);
    await new Promise((resolve) => setTimeout(resolve));
    expect(messages).toEqual([
      { source: SWITCH_MESSAGE_SOURCE, backward: false },
      { source: SWITCH_MESSAGE_SOURCE, backward: true },
    ]);
  });

  it("keeps a held Ctrl+Tab from the page but relays only the first press", async () => {
    const { window, messages } = await render("<p>x</p>", false, { relaySwitchKeys: true });
    expect(press(window, { key: "Tab", ctrlKey: true }).defaultPrevented).toBe(true);
    expect(press(window, { key: "Tab", ctrlKey: true, repeat: true }).defaultPrevented).toBe(true);
    await new Promise((resolve) => setTimeout(resolve));
    expect(messages).toEqual([{ source: SWITCH_MESSAGE_SOURCE, backward: false }]);
  });

  it("relays no Tab but Ctrl+Tab, and none elsewhere", async () => {
    const { window, messages } = await render("<p>x</p>", false, { relaySwitchKeys: true });
    for (const init of [
      { key: "Tab" },
      { key: "Tab", shiftKey: true },
      { key: "Tab", ctrlKey: true, altKey: true },
      { key: "Tab", ctrlKey: true, metaKey: true },
      { key: "Tab", ctrlKey: true, isComposing: true },
      { key: "a", ctrlKey: true },
    ]) {
      expect(press(window, init).defaultPrevented).toBe(false);
    }
    const elsewhere = await render("<p>x</p>", false, { relayHistoryKeys: true });
    expect(press(elsewhere.window, { key: "Tab", ctrlKey: true }).defaultPrevented).toBe(false);
    await new Promise((resolve) => setTimeout(resolve));
    expect(messages).toEqual([]);
    expect(elsewhere.messages).toEqual([]);
  });
});

describe("the composed document", () => {
  it("nonces only Octoboard's own scripts, and keeps the page out of its markup", () => {
    const doc = composePageDocument(`</script><script>evil()</script><p>x</p>`, false, NONCE);
    expect(doc).toContain(`script-src 'nonce-${NONCE}'`);
    expect(doc).not.toContain("script-src 'unsafe-inline'");
    expect(doc.match(/<script/g)).toHaveLength(3);
    expect(doc.match(/<script nonce="0123456789abcdef">/g)).toHaveLength(3);
    expect(doc).not.toContain("evil()</script>");
    expect(doc).toContain(String.raw`</script>`);
  });
});

describe("submissionFromEntries", () => {
  it("builds the submitted object in document order, joining a repeated name", () => {
    const data = submissionFromEntries([
      ["b", "1"],
      ["a", "x"],
      ["a", "y"],
      ["__proto__", "p"],
    ]);
    expect(Object.keys(data!)).toEqual(["b", "a", "__proto__"]);
    expect(data!.a).toBe("x, y");
    expect(submissionFromEntries([["a", 1]])).toBeUndefined();
    expect(submissionFromEntries("a")).toBeUndefined();
  });
});
