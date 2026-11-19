// What `screenshots.test.ts` runs the capture script against, in place of a browser, a dev server and the machine-wide
// heavy-step slot. The file is both the loader hook that redirects the imports of the script (run from a copy, so that
// what it writes lands in a temporary directory) and the module the redirected imports land on, which `vite`,
// Playwright and the slot queue are, and what it does is set by environment variables of the process under test:
//
//   GALLERY_TEST_BROWSER   the real `gallery-browser.mjs`, which the copied script and manifest module no longer
//                          reach by their relative import
//   GALLERY_TEST_SCENARIOS JSON, the scenarios the gallery offers: [{ id, width?, minWidth? }]
//   GALLERY_TEST_MODE      one of the failures below; unset, every page loads and every capture works. The `clip-*`
//                          modes apply to the clip selectors, the `hover-*` ones to the selector `[data-hover]`
//   GALLERY_TEST_FAIL      with a mode, the scenario the failure is limited to (default: all)
//   GALLERY_TEST_BOX       JSON, the bounding box every clip selector finds ({ x, y, width, height })
//
// The fake page logs what it was asked for as `fake: ...` lines, and a screenshot is the JSON of the options it was
// given, so the test can read the geometry back from the written file.
import { Buffer } from "node:buffer";
import process from "node:process";

const here = import.meta.url;
const mode = process.env.GALLERY_TEST_MODE;
const failing = (url) => Boolean(mode) && (!process.env.GALLERY_TEST_FAIL || url.includes(`scenario=${process.env.GALLERY_TEST_FAIL}&`));

export async function resolve(specifier, context, nextResolve) {
  if (/\/packages\/ui\/scripts\/gallery-browser\.mjs$/.test(specifier)) return { url: process.env.GALLERY_TEST_BROWSER, shortCircuit: true };
  if (/\/scripts\/verify\/lock\.mjs$/.test(specifier) || specifier === "vite" || /\/node_modules\/playwright(?:-core)?\//.test(specifier)) {
    return { url: here, shortCircuit: true };
  }
  return nextResolve(specifier, context);
}

export async function acquireSlot(label) {
  console.log(`fake: slot acquired for ${label}`);
  return () => console.log("fake: slot released");
}

export const describeWait = () => "fake wait";

export async function createServer() {
  return {
    async listen() {},
    httpServer: { address: () => ({ port: 1234 }) },
    close: () => console.log("fake: closed server"),
  };
}

function newPage() {
  let url = "";
  return {
    on(event, listener) {
      if (event === "pageerror" && mode === "page-error") listener(new Error("boom\n    at stack line"));
    },
    async goto(target) {
      url = target;
      console.log(`fake: goto ${target}`);
      if (mode === "hang-capture" && target.includes("lang=en")) {
        // Keeps the process alive, as a page that is still loading does, until the test sends its signal.
        console.log("fake: ready for signal");
        setInterval(() => {}, 1_000);
        await new Promise(() => {});
      }
    },
    async waitForSelector() {
      if (mode === "never-ready" && failing(url)) throw Object.assign(new Error("Timeout 60000ms exceeded"), { name: "TimeoutError" });
      if (mode === "ready-error" && failing(url)) throw new Error("net::ERR_CONNECTION_REFUSED");
    },
    async waitForTimeout(ms) {
      console.log(`fake: wait ${ms}`);
    },
    mouse: {
      async move(x, y) {
        console.log(`fake: mouse move ${x} ${y}`);
      },
    },
    async evaluate(fn) {
      if (String(fn).includes("SCENARIOS")) {
        if (mode === "gallery-unreadable") throw new Error("import of the fixtures failed\n    at stack line");
        return { scenarios: JSON.parse(process.env.GALLERY_TEST_SCENARIOS ?? "[]"), languages: ["en"], catalogLanguages: ["en"] };
      }
      return mode === "banner-failure" && failing(url) ? ["fixture failed", "second line"] : [];
    },
    locator(selector) {
      console.log(`fake: locator ${selector}`);
      const kind = selector === "[data-hover]" ? "hover" : "clip";
      return {
        // What `isolate` runs in the page, with the padding it is given.
        evaluate: async (_fn, padding) => console.log(`fake: isolate ${selector} ${padding}`),
        // What `isolate` runs to mark the elements a `keep` selector names: how many it found.
        evaluateAll: async () => {
          console.log(`fake: keep ${selector}`);
          return mode === "keep-none" ? 0 : 1;
        },
        count: async () => (mode === `${kind}-none` ? 0 : mode === `${kind}-two` ? 2 : 1),
        boundingBox: async () => (mode === `${kind}-hidden` ? null : JSON.parse(process.env.GALLERY_TEST_BOX ?? '{"x":50,"y":40,"width":300,"height":200}')),
      };
    },
    async screenshot(options) {
      return Buffer.from(JSON.stringify(options));
    },
    async close() {},
  };
}

export const chromium = {
  async launch(options) {
    console.log(`fake: launch ${options?.executablePath ?? "default"}`);
    return {
      async newPage() { return newPage(); },
      async newContext(options) {
        console.log(`fake: context ${JSON.stringify(options)}`);
        return { newPage: async () => newPage(), async close() {} };
      },
      close: () => console.log("fake: closed browser"),
    };
  },
};
