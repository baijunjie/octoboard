// Injected before the page's own scripts. Records animation frames, terminal-socket traffic,
// control-socket requests and React commits, and offers the waits, the lookups and the press the
// scenarios build on. It only observes: the one thing it reads from the app is the terminal's
// text view, which the WebGL renderer leaves out of the DOM.
(() => {
  const P = (window.__perf = { frames: [], ws: [], commits: [], reqs: [], ctlIn: [] });
  const loop = () => {
    P.frames.push(performance.now());
    requestAnimationFrame(loop);
  };
  requestAnimationFrame(loop);

  // A frame count every 5 s tells a stalled run from a page that never loaded: whoever runs the page
  // reads the beat and the count to say which. It goes out through the same handler as the results.
  setInterval(() => window.webkit?.messageHandlers?.bench?.postMessage(JSON.stringify({ heartbeat: P.frames.length })), 5000);

  P.until = async (fn, ms = 30000, what = "the page to show its fixtures") => {
    const start = performance.now();
    while (!fn()) {
      if (performance.now() - start > ms) throw new Error(`timeout waiting for ${what}`);
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
  };

  // The accessible names carry Unicode isolation marks around the console and session names.
  P.avatar = (n) => [...document.querySelectorAll("button")].find((b) => new RegExp(`^⁨Console ${n}⁩`).test(b.getAttribute("aria-label") ?? b.textContent));
  P.card = (name) => [...document.querySelectorAll("[aria-label]")].find((e) => e.getAttribute("aria-label").startsWith(`⁨${name}⁩ session`));
  // The pointer events of a mouse click on `el`; `rect` is its box, which a caller timing the press reads first.
  P.press = (el, rect = el.getBoundingClientRect()) => {
    const o = { bubbles: true, cancelable: true, composed: true, clientX: rect.x + rect.width / 2, clientY: rect.y + rect.height / 2, button: 0, pointerId: 1, pointerType: "mouse", isPrimary: true, view: window };
    el.dispatchEvent(new PointerEvent("pointerover", o));
    el.dispatchEvent(new PointerEvent("pointerdown", { ...o, buttons: 1 }));
    el.dispatchEvent(new MouseEvent("mousedown", { ...o, buttons: 1 }));
    el.dispatchEvent(new PointerEvent("pointerup", o));
    el.dispatchEvent(new MouseEvent("mouseup", o));
    el.dispatchEvent(new MouseEvent("click", o));
  };
  // The driver's own forced layout, which the probe below leaves out of its attribution.
  let own = false;
  P.layoutRead = () => {
    own = true;
    try { return document.body.offsetHeight; } finally { own = false; }
  };

  const NativeWS = window.WebSocket;
  window.WebSocket = class extends NativeWS {
    constructor(url, protocols) {
      super(url, protocols);
      const u = String(url);
      if (u.includes("/ws/control")) {
        // The requests the page sends, and when the reply carrying the same id comes back.
        const send = this.send.bind(this);
        this.send = (data) => {
          try {
            const m = JSON.parse(data);
            if (m.id) P.reqs.push({ id: m.id, type: m.type, t: performance.now() });
          } catch { /* not JSON: not a request */ }
          return send(data);
        };
        this.addEventListener("message", (e) => {
          const t = performance.now();
          try {
            const m = JSON.parse(e.data);
            P.ctlIn.push([t, m.type]);
            if (m.id) {
              const r = P.reqs.find((q) => q.id === m.id && q.reply === undefined);
              if (r) r.reply = t;
            }
          } catch { /* not JSON */ }
        });
        return;
      }
      if (!u.includes("/ws/term/")) return;
      const rec = { msgs: [] };
      P.ws.push(rec);
      this.addEventListener("message", (e) => rec.msgs.push([performance.now(), e.data.byteLength ?? e.data.length]));
    }
  };

  // React reports every commit to a DevTools hook when one is present, production builds included.
  window.__REACT_DEVTOOLS_GLOBAL_HOOK__ = {
    supportsFiber: true, renderers: new Map(), isDisabled: false,
    inject() { return 1; }, checkDCE() {},
    onCommitFiberRoot() { P.commits.push(performance.now()); },
    onCommitFiberUnmount() {}, onPostCommitFiberRoot() {}, onScheduleFiberRoot() {},
  };

  // Resolves once no frame was late by more than `gapMs` for `quietMs`: a main thread that is busy
  // (script, style, layout) cannot deliver the next frame on time, so a late frame is a busy one.
  P.waitIdle = (quietMs = 500, gapMs = 34, timeoutMs = 20000) =>
    new Promise((resolve) => {
      const start = performance.now();
      let lastBusy = start;
      let prev = start;
      const step = (t) => {
        if (t - prev > gapMs) lastBusy = t;
        prev = t;
        if (t - lastBusy >= quietMs || t - start > timeoutMs) resolve({ idleAt: lastBusy, timedOut: t - start > timeoutMs });
        else requestAnimationFrame(step);
      };
      requestAnimationFrame(step);
    });

  // The stand-in agent prints its pid once, in the status line under the transcript, after its spinner has
  // stopped redrawing: the pid in the terminal's rows says which session the terminal shows and that the
  // end of its replay has been parsed. The text is read from xterm's buffer through the read-only view the
  // controller hangs on the terminal's element, since the WebGL renderer draws no row elements.
  P.terminalText = () => {
    const view = document.querySelector('[data-region="terminal"] .xterm')?.octoboardTerminalText;
    if (!view) return "";
    let text = "";
    for (let y = 0; y < view.rows(); y++) text += view.row(y) + "\n";
    return text;
  };
  P.terminalPid = () => /pid (\d+)/.exec(P.terminalText())?.[1];
  // Resolves, with the time or `null` on timeout, once the terminal shows a session other than `before`.
  P.waitTerminalChange = (before, timeoutMs = 30000) =>
    new Promise((resolve) => {
      const start = performance.now();
      const check = () => {
        const pid = P.terminalPid();
        if (pid !== undefined && pid !== before) resolve(performance.now());
        else if (performance.now() - start > timeoutMs) resolve(null);
        else requestAnimationFrame(check);
      };
      check();
    });

  // The measurements of the window [t0, t1] one step reports.
  P.summarize = (t0, t1) => {
    const frames = P.frames.filter((t) => t >= t0 && t <= t1 + 1);
    const gaps = frames.slice(1).map((t, i) => t - frames[i]);
    const sorted = [...gaps].sort((a, b) => a - b);
    const q = (p) => (sorted.length ? +sorted[Math.min(sorted.length - 1, Math.floor(p * sorted.length))].toFixed(1) : null);
    const ctl = {};
    for (const [t, type] of P.ctlIn) if (t >= t0 && t <= t1) ctl[type] = (ctl[type] ?? 0) + 1;
    return {
      frames: gaps.length, gapP50: q(0.5), gapP95: q(0.95), gapMax: q(1),
      longGaps: gaps.filter((g) => g > 50).map((g) => +g.toFixed(0)),
      commits: P.commits.filter((t) => t >= t0 && t <= t1).length,
      wsBytes: P.ws.flatMap((r) => r.msgs).filter(([t]) => t >= t0 && t <= t1).reduce((a, [, n]) => a + n, 0),
      reqs: P.reqs.filter((r) => r.t >= t0 - 5 && r.t <= t1).map((r) => `${r.type}:${r.reply ? (r.reply - r.t).toFixed(0) + "ms" : "pending"}`),
      ctlIn: ctl,
    };
  };

  // `?probe=1`: times every layout-reading DOM API call. A read that has to bring style and layout up to
  // date first is slow, so its time is the forced style and layout it caused, attributed to its caller;
  // the stacks are readable on a build made with `--minify false`.
  if (!new URLSearchParams(location.search).has("probe")) return;
  P.reads = [];
  const record = (name, t, d) => {
    if (d < 1 || own) return;
    const stack = (new Error().stack ?? "").split("\n").slice(2, 7).map((l) => l.replace(/https?:\/\/[^/]+\/assets\//, "").trim()).join(" < ");
    P.reads.push([t, d, name, stack]);
  };
  const wrapGetter = (proto, prop) => {
    const desc = Object.getOwnPropertyDescriptor(proto, prop);
    if (!desc || !desc.get) return;
    Object.defineProperty(proto, prop, { ...desc, get() { const t = performance.now(); const v = desc.get.call(this); record(prop, t, performance.now() - t); return v; } });
  };
  for (const p of ["offsetTop", "offsetLeft", "offsetWidth", "offsetHeight", "offsetParent"]) wrapGetter(HTMLElement.prototype, p);
  for (const p of ["scrollLeft", "scrollTop", "scrollWidth", "scrollHeight", "clientWidth", "clientHeight"]) wrapGetter(Element.prototype, p);
  const wrapFn = (proto, name) => {
    const f = proto[name];
    proto[name] = function (...a) { const t = performance.now(); const v = f.apply(this, a); record(name, t, performance.now() - t); return v; };
  };
  wrapFn(Element.prototype, "getBoundingClientRect");
  wrapFn(Element.prototype, "getClientRects");
  wrapFn(window, "getComputedStyle");
  // getComputedStyle itself is lazy; the cost lands on the property read.
  const style = CSSStyleDeclaration.prototype;
  const getPropertyValue = style.getPropertyValue;
  style.getPropertyValue = function (n) { const t = performance.now(); const v = getPropertyValue.call(this, n); record("getPropertyValue", t, performance.now() - t); return v; };
  P.readSummary = (t0, t1) => {
    const reads = P.reads.filter(([t]) => t >= t0 && t <= t1);
    const byStack = {};
    for (const [, d, name, stack] of reads) {
      const k = `${name} @ ${stack.split(" < ").slice(0, 3).join(" < ")}`;
      byStack[k] = byStack[k] ?? [0, 0];
      byStack[k][0] += d;
      byStack[k][1]++;
    }
    return {
      forcedMs: +reads.reduce((a, [, d]) => a + d, 0).toFixed(1),
      slowReads: reads.length,
      top: Object.entries(byStack).sort((a, b) => b[1][0] - a[1][0]).slice(0, 8).map(([k, [d, n]]) => `${d.toFixed(1)}ms x${n} ${k}`),
    };
  };
})();
