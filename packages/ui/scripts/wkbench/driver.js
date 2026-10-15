// The in-page scenario driver: dispatches the user's real input (pointer events on a control, wheel
// events on the terminal) and times it, then posts one JSON line per step to
// `webkit.messageHandlers.bench`. Chosen by the page URL's query: `scenario=switch|scroll|idle` (default
// switch), `reps` (switch), `session` and `delta` (scroll).
(async () => {
  const P = window.__perf;
  const log = (o) => window.webkit.messageHandlers.bench.postMessage(JSON.stringify(o));
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const { avatar, card } = P;
  try {
    await P.until(() => avatar(1) && card("p1-s1"));
    await sleep(1500);
    const params = new URLSearchParams(location.search);
    // One timed step: press `target()`, then wait until the terminal shows another session (`attach`)
    // and the main thread has been idle for 500 ms. `toIdle` is from the press to the last late frame.
    const step = async (name, target, attach) => {
      await P.waitIdle(400);
      const before = P.terminalPid();
      const el = target();
      const rect = el.getBoundingClientRect();
      const t0 = performance.now();
      // The synchronous part: the press handlers and React's render and commit of a discrete update,
      // which flushes in a microtask queued during the event; then a forced style and layout pass of
      // whatever is left dirty.
      P.press(el, rect);
      for (let i = 0; i < 3; i++) await Promise.resolve();
      const t1 = performance.now();
      const commitsInPress = P.commits.filter((t) => t >= t0).length;
      P.layoutRead();
      const t2 = performance.now();
      // The next frame's callbacks run after style, layout and paint of this one.
      const tFrame = await new Promise((r) => requestAnimationFrame(() => setTimeout(() => r(performance.now()), 0)));
      const tAttached = attach ? await P.waitTerminalChange(before) : undefined;
      const { idleAt, timedOut } = await P.waitIdle(500);
      const end = Math.max(idleAt, tAttached ?? 0);
      log({
        step: name, pressJs: +(t1 - t0).toFixed(1), commitsInPress, forcedStyleLayout: +(t2 - t1).toFixed(1),
        toNextFrame: +(tFrame - t0).toFixed(1), toTerminal: tAttached && +(tAttached - t0).toFixed(1),
        attachTimedOut: tAttached === null || undefined, idleTimedOut: timedOut || undefined, toIdle: +(end - t0).toFixed(1),
        ...P.summarize(t0, end), ...(P.readSummary ? { forced: P.readSummary(t0, end) } : {}),
      });
    };
    const scenario = params.get("scenario") ?? "switch";
    if (scenario === "idle") {
      // The page left alone for 5 s while the daemon broadcasts (the pump renames sessions meanwhile).
      for (const n of [2, 1]) {
        await step(`avatar→C${n}`, () => avatar(n), false);
        const t0 = performance.now();
        await sleep(5000);
        const t1 = performance.now();
        const frames = P.frames.filter((t) => t >= t0 && t <= t1);
        const late = frames.slice(1).map((t, i) => t - frames[i]).reduce((a, g) => a + Math.max(0, g - 16.7), 0);
        log({ step: `idle 5s showing C${n}`, droppedMs: +late.toFixed(0), ...P.summarize(t0, t1) });
      }
    } else if (scenario === "switch") {
      await step("select p1-s1 (warm-up attach)", () => card("p1-s1"), true);
      for (let rep = 0; rep < Number(params.get("reps") ?? 3); rep++) {
        await step("avatar→C2", () => avatar(2), false);
        await step("select p2-s1 (attach)", () => card("p2-s1"), true);
        await step("select p2-s2 (attach, same console)", () => card("p2-s2"), true);
        await step("avatar→C3", () => avatar(3), false);
        await step("select p3-s1 (attach)", () => card("p3-s1"), true);
        await step("avatar→C1", () => avatar(1), false);
        await step("select p1-s1 (attach)", () => card("p1-s1"), true);
      }
    } else if (scenario === "scroll") {
      const target = params.get("session") ?? "p2-s1";
      await step(`avatar→C${target[1]}`, () => avatar(target[1]), false);
      await step(`select ${target}`, () => card(target), true);
      const screen = document.querySelector(".xterm-screen");
      const r = screen.getBoundingClientRect();
      for (const dir of [-1, 1]) {
        await P.waitIdle(400);
        const t0 = performance.now();
        // A trackpad-like stream: one wheel event per frame for 3 s.
        await new Promise((resolve) => {
          const tick = () => {
            if (performance.now() - t0 > 3000) return resolve();
            screen.dispatchEvent(new WheelEvent("wheel", { bubbles: true, cancelable: true, deltaY: dir * Number(params.get("delta") ?? 40), deltaMode: 0, clientX: r.x + r.width / 2, clientY: r.y + r.height / 2 }));
            requestAnimationFrame(tick);
          };
          requestAnimationFrame(tick);
        });
        log({ step: `wheel ${dir < 0 ? "up" : "down"} 3s (1 event/frame)`, ...P.summarize(t0, performance.now()) });
      }
    } else {
      throw new Error(`unknown scenario "${scenario}"`);
    }
    log({ done: true });
  } catch (e) {
    log({ error: String((e && e.stack) || e) });
    log({ done: true });
  }
})();
