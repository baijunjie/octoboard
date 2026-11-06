/** Compares a union-sized RGBA canvas: a pixel absent on either side always differs, even if transparent. */
export function comparePixels(base, current, threshold = 8) {
  const width = Math.max(base.width, current.width), height = Math.max(base.height, current.height);
  const data = new Uint8ClampedArray(width * height * 4);
  let count = 0, left = width, top = height, right = -1, bottom = -1;
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const a = (y * base.width + x) * 4, b = (y * current.width + x) * 4, out = (y * width + x) * 4;
      const hasBase = x < base.width && y < base.height, hasCurrent = x < current.width && y < current.height;
      const changed = hasBase !== hasCurrent || (hasBase && hasCurrent && [0, 1, 2, 3].some((c) => Math.abs(base.data[a + c] - current.data[b + c]) > threshold));
      if (changed) {
        count++; left = Math.min(left, x); top = Math.min(top, y); right = Math.max(right, x); bottom = Math.max(bottom, y);
      }
      data[out] = changed ? 255 : Math.round((hasCurrent ? current.data[b] : 255) * 0.4 + 153);
      data[out + 1] = changed ? 0 : Math.round((hasCurrent ? current.data[b + 1] : 255) * 0.4 + 153);
      data[out + 2] = changed ? 0 : Math.round((hasCurrent ? current.data[b + 2] : 255) * 0.4 + 153);
      data[out + 3] = 255;
    }
  }
  return { width, height, data, pixels: count, percent: count / (width * height) * 100, bounds: count ? { left, top, right, bottom } : null };
}

export function selectScenarios(base, current, { patterns = [], group } = {}) {
  const all = new Map(base.map((s) => [s.id, { id: s.id, base: s }]));
  for (const s of current) all.set(s.id, { ...all.get(s.id), id: s.id, current: s });
  let selected = [...all.values()].filter((s) => !group || [s.base?.group, s.current?.group].some((g) => g?.toLowerCase().includes(group.toLowerCase())));
  const matchers = patterns.map((pattern) => ({ pattern, re: new RegExp(`^${pattern.split("*").map((part) => part.replace(/[.+?^${}()|[\]\\]/g, "\\$&")).join(".*")}$`) }));
  for (const { pattern, re } of matchers) if (!selected.some((s) => re.test(s.id))) throw new Error(`No scenario matches: ${pattern}`);
  if (matchers.length) selected = selected.filter((s) => matchers.some(({ re }) => re.test(s.id)));
  if (!selected.length) throw new Error("No scenarios selected");
  return selected;
}

/** Both sides use the current scenario's viewport, widened for either side's minimum, so a layout change is visible. */
export function viewportOf(pair, width) {
  const scenario = pair.current ?? pair.base;
  return { width: Math.max(scenario.width ?? width ?? 1440, pair.base?.minWidth ?? 0, pair.current?.minWidth ?? 0), height: Math.max(400, scenario.height ?? 900) };
}

const escape = (value) => String(value ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

export function renderReport(report) {
  const count = (status) => report.results.filter((r) => r.status === status).length;
  const images = (r) => ["base", "current", "diff"].map((side) => r.images?.[side] ? `<figure><figcaption>${side}</figcaption><a href="${escape(r.images[side])}"><img loading="lazy" src="${escape(r.images[side])}" alt="${side} screenshot"></a></figure>` : "").join("");
  return `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Gallery comparison</title>
<style>body{font:15px system-ui;margin:2rem;background:#f5f5f5;color:#202020}a{color:#1659a0}header,article{background:white;padding:1rem;margin-bottom:1rem;border:1px solid #ccc;border-radius:8px}h1{margin-top:0}code{overflow-wrap:anywhere}.images{display:flex;gap:1rem;overflow:auto}figure{margin:0;flex:1;min-width:240px}img{width:100%;border:1px solid #ddd}figcaption{text-transform:capitalize;font-weight:bold;margin-bottom:.5rem}.changed,.added,.removed{border-left:5px solid #ba7a00}.error{border-left:5px solid #bb2828}pre{white-space:pre-wrap;overflow-wrap:anywhere}summary{cursor:pointer}nav a{margin-right:1rem}</style>
<header><h1>Gallery comparison</h1><p>${escape(report.startedAt)} · ${report.complete ? "Complete" : "Incomplete"}</p><p>Base: <code>${escape(report.baseUrl)}</code><br>Current: <code>${escape(report.url)}</code></p>
<p>${report.results.length} results: ${["unchanged", "changed", "added", "removed", "error", "pending"].map((s) => `${count(s)} ${s}`).join(" · ")}</p><p>Channel difference threshold: ${escape(report.threshold)} / 255. Differences need human review; rendering and fixture changes can also change pixels.</p>${report.error ? `<pre>${escape(report.error)}</pre>` : ""}<a href="results.json">JSON report</a></header>
${report.results.map((r, i) => `<article class="${escape(r.status)}" id="case-${i}"><details${r.status !== "unchanged" ? " open" : ""}><summary><strong>${escape(r.id)}</strong> · ${escape(r.status)} · ${escape(r.language)} / ${escape(r.theme)} · ${r.viewport.width} × ${r.viewport.height}${r.pixels !== undefined ? ` · ${r.pixels} pixels (${r.percent.toFixed(3)}%)` : ""}</summary><p>${escape(r.title)}</p>${r.bounds ? `<p>Difference bounds: ${escape(JSON.stringify(r.bounds))}</p>` : ""}${r.error ? `<pre>${escape(r.error)}</pre>` : ""}<div class="images">${images(r)}</div></details></article>`).join("\n")}</html>`;
}
