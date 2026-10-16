// Calibrates what one forced style recalculation plus layout costs, for the whole document and for each
// region, and how many elements each region has: a custom property set on a region restyles what is
// under it, a 1px change of the region's padding dirties its layout, and the read of `offsetHeight`
// makes both passes run there and then. Posts one result line, then done.
(async () => {
  const log = (o) => window.webkit.messageHandlers.bench.postMessage(JSON.stringify(o));
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const P = window.__perf;
  try {
    await P.until(() => P.card("p1-s1"));
    await sleep(1000);
    P.press(P.card("p1-s1"));
    // The terminal fills with the session's replay; calibrate on the page as it is once that is done.
    await P.until(() => P.terminalPid() !== undefined, 30000, "the terminal to show the session");
    await P.waitIdle(500);
    const regions = {
      document: "html", sidebar: '[data-region="sidebar"]', rail: '[data-region="rail"]',
      terminal: '[data-region="terminal"]', xtermScreen: ".xterm-screen", aside: '[data-region="aside"]',
    };
    const elements = (sel) => { const el = document.querySelector(sel); return el ? el.querySelectorAll("*").length + 1 : 0; };
    const counts = Object.fromEntries(Object.entries(regions).map(([k, s]) => [k, elements(s)]));
    let rules = 0;
    for (const sheet of document.styleSheets) { try { rules += sheet.cssRules.length; } catch { /* a cross-origin sheet */ } }
    // The median of `n` passes, each with different values so the property and the padding really change.
    const time = (el, n = 7) => {
      const ts = [];
      for (let i = 0; i < n; i++) {
        el.style.setProperty("--wkbench-probe", String(i + Math.random()));
        el.style.paddingRight = `${(i % 2) + 1}px`;
        const t = performance.now();
        document.body.offsetHeight;
        ts.push(performance.now() - t);
      }
      el.style.removeProperty("--wkbench-probe");
      el.style.removeProperty("padding-right");
      document.body.offsetHeight; // settle the page before the next region
      ts.sort((a, b) => a - b);
      return +ts[Math.floor(n / 2)].toFixed(1);
    };
    const costs = {};
    for (const [k, s] of Object.entries(regions)) { const el = document.querySelector(s); costs[k] = el ? time(el) : null; }
    // What the terminal adds to the document's cost: under the DOM renderer, one element per row under
    // `.xterm-screen`; under WebGL, just the canvases, so the two numbers (this and the region's own
    // `xtermScreen` count) say which renderer the run measured.
    const screen = document.querySelector(".xterm-screen");
    screen.style.display = "none";
    const withoutScreen = time(document.documentElement);
    screen.style.display = "";
    log({ step: "restyle", counts, cssRules: rules, restyleLayoutMs: costs, documentWithoutXtermScreenMs: withoutScreen, dpr: devicePixelRatio, ua: navigator.userAgent });
  } catch (e) { log({ error: String(e.stack || e) }); }
  log({ done: true });
})();
