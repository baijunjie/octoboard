const mode = process.env.GALLERY_COMPARE_TEST_MODE;
const png = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLttAAAAABJRU5ErkJggg==", "base64");
let directPages = 0, pendingTimer;
const page = (direct = 0) => ({
  goto: async () => {}, close: async () => {
    if ((mode === "read-close-hang" && direct === 1) || (mode === "diff-close-hang" && direct === 3)) await new Promise(() => {});
  }, on: () => {}, addScriptTag: async () => {}, waitForTimeout: async () => {},
  waitForSelector: async () => {
    if (mode === "render-error") throw new Error("fixture render timeout");
    if (mode === "interrupt-capture") { console.log("ready to interrupt capture"); await new Promise((resolve) => { pendingTimer = setTimeout(resolve, 60_000); }); }
  },
  locator: () => ({ allTextContents: async () => [] }), screenshot: async () => { console.log("captured fixture image"); return png; },
  evaluate: async (fn) => {
    if (String(fn).includes("SCENARIOS")) return { languages: ["en"], scenarios: [{ id: "test", title: "Test", group: "Tests" }, ...(mode === "diff-close-hang" ? [{ id: "test-second", title: "Second", group: "Tests" }] : [])] };
    if (String(fn).includes("galleryComparePixels")) return { width: 1, height: 1, pixels: mode === "changed" ? 1 : 0, percent: mode === "changed" ? 100 : 0, bounds: null, png: png.toString("base64") };
  },
});
export const chromium = { launch: async () => {
  console.log("opened browser");
  return { newPage: async () => page(++directPages), newContext: async () => ({ newPage: async () => page(), close: async () => { if (mode === "context-close-error") throw new Error("fixture context close failed"); } }), close: async () => { clearTimeout(pendingTimer); console.log("closed browser"); } };
} };
