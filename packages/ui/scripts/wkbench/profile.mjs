// A Chrome CPU profile of a repeated UI step, attributed by original source file (through the source
// maps of a build made with `--minify false --sourcemap`) and by React component (inclusive time). It is
// the attribution path: the system WKWebView says how long a step takes, this says which code in it.
// Read the shares, not the times.
//
//   node profile.mjs --url <ui url> --dist <the unminified build's directory> [--step avatar|attach]
//                    [--reps 6] [--throttle 1] [--chromium <path>]
//
// It is runnable on its own against any served build and daemon.
import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { launch, newBenchPage } from "./playwright.mjs";

/** The profile of `reps` rounds of the step across the three consoles, printed to stdout. */
export async function profileSteps({ url, dist, step = "avatar", reps = 6, throttle, chromium }) {
  // source-map-js is a dependency of Vite's PostCSS, not of this package: resolve it from Vite.
  const requireFromVite = createRequire(createRequire(import.meta.url).resolve("vite/package.json"));
  const { SourceMapConsumer } = requireFromVite("source-map-js");
  const browser = await launch("chromium", { chromium });
  const profiles = [];
  try {
    const { page, cdp } = await newBenchPage(browser);
    await page.goto(url);
    const avatar = (n) => page.getByRole("navigation", { name: "Consoles and window controls" }).getByRole("button", { name: new RegExp(`^⁨Console ${n}⁩`) });
    const card = (name) => page.getByRole("button", { name: new RegExp(`^⁨${name}⁩ session`) });
    const idle = () => page.evaluate(() => window.__perf.waitIdle(400));
    const stopProfile = async () => (await cdp.send("Profiler.stop")).profile;
    await avatar(1).waitFor();
    await card("p1-s1").click();
    await page.evaluate(() => window.__perf.waitTerminalChange(undefined));
    await idle();
    if (throttle) await cdp.send("Emulation.setCPUThrottlingRate", { rate: Number(throttle) });
    await cdp.send("Profiler.enable");
    await cdp.send("Profiler.setSamplingInterval", { interval: 100 });
    await cdp.send("Profiler.start");
    for (let r = 0; r < reps; r++) {
      if (step === "avatar") {
        for (const n of [2, 3, 1]) { await avatar(n).click(); await idle(); }
      } else {
        for (const [n, session] of [[2, "p2-s1"], [3, "p3-s1"], [1, "p1-s1"]]) {
          await avatar(n).click();
          await idle();
          // Keep only the attach in the profile.
          await stopProfile();
          await cdp.send("Profiler.start");
          const before = await page.evaluate(() => window.__perf.terminalPid());
          await card(session).click();
          await page.evaluate((b) => window.__perf.waitTerminalChange(b), before);
          await idle();
          profiles.push(await stopProfile());
          await cdp.send("Profiler.start");
        }
      }
    }
    const last = await stopProfile();
    if (step === "avatar") profiles.push(last);
  } finally {
    await browser.close();
  }

  const maps = new Map();
  const mapFor = (scriptUrl) => {
    const file = scriptUrl.split("/").pop();
    if (!file.endsWith(".js")) return null;
    if (!maps.has(file)) {
      const mapPath = path.join(dist, "assets", `${file}.map`);
      maps.set(file, fs.existsSync(mapPath) ? new SourceMapConsumer(JSON.parse(fs.readFileSync(mapPath, "utf8"))) : null);
    }
    return maps.get(file);
  };
  const bySource = new Map();
  const byFunction = new Map();
  const byComponent = new Map();
  let busy = 0;
  let idle = 0;
  for (const profile of profiles) {
    const nodes = new Map(profile.nodes.map((n) => [n.id, n]));
    const parent = new Map();
    for (const n of profile.nodes) for (const c of n.children ?? []) parent.set(c, n.id);
    const ours = (src) => src.includes("packages/ui/src/") || src.startsWith("src/");
    const origin = (frame) => {
      const consumer = frame.url && mapFor(frame.url);
      if (!consumer) return frame.url ? frame.url.split("/").pop() : "(native)";
      const o = consumer.originalPositionFor({ line: frame.lineNumber + 1, column: frame.columnNumber });
      if (!o.source) return frame.url.split("/").pop();
      return o.source.replace(/^.*node_modules\/\.pnpm\/[^/]+\/node_modules\//, "").replace(/^(\.\.\/)+/, "");
    };
    profile.samples.forEach((id, i) => {
      const ms = (profile.timeDeltas[i] ?? 0) / 1000;
      const frame = nodes.get(id).callFrame;
      if (frame.functionName === "(idle)") { idle += ms; return; }
      busy += ms;
      const src = origin(frame);
      const pkg = ours(src)
        ? src.replace(/^.*packages\/ui\//, "")
        : src.split("/").slice(0, src.startsWith("@") ? 2 : 1).join("/");
      bySource.set(pkg, (bySource.get(pkg) ?? 0) + ms);
      const key = `${frame.functionName || "(anon)"} [${src.split("/").slice(-2).join("/")}]`;
      byFunction.set(key, (byFunction.get(key) ?? 0) + ms);
      // Inclusive time per component of our own source: walk up to every capitalized function defined
      // under src/, once each.
      const seen = new Set();
      for (let cur = id; cur !== undefined; cur = parent.get(cur)) {
        const f = nodes.get(cur).callFrame;
        if (/^[A-Z]/.test(f.functionName) && f.url.includes("/assets/") && ours(origin(f)) && !seen.has(f.functionName)) {
          seen.add(f.functionName);
          byComponent.set(f.functionName, (byComponent.get(f.functionName) ?? 0) + ms);
        }
      }
    });
  }
  const top = (m, n) => [...m.entries()].sort((a, b) => b[1] - a[1]).slice(0, n).map(([k, v]) => `${v.toFixed(1).padStart(8)} ms  ${k}`).join("\n");
  console.log(`profiles=${profiles.length} busy=${busy.toFixed(1)}ms idle=${idle.toFixed(1)}ms`);
  console.log(`-- self time by origin\n${top(bySource, 18)}`);
  console.log(`-- self time by function\n${top(byFunction, 30)}`);
  console.log(`-- inclusive time by our components (render phase only)\n${top(byComponent, 20)}`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const { values } = parseArgs({ options: { url: { type: "string" }, dist: { type: "string" }, step: { type: "string" }, reps: { type: "string" }, throttle: { type: "string" }, chromium: { type: "string" } } });
  if (!values.url || !values.dist) {
    console.error("usage: node profile.mjs --url <ui url> --dist <dir> [--step avatar|attach] [--reps 6] [--throttle 1] [--chromium <path>]");
    process.exit(2);
  }
  await profileSteps({ url: values.url, dist: values.dist, step: values.step, reps: values.reps && Number(values.reps), throttle: values.throttle, chromium: values.chromium });
}
