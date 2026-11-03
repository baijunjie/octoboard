const resolved = new Map<string, string | undefined>();
// `null` once the browser has refused a canvas, so it is not asked again on every call.
let canvas: CanvasRenderingContext2D | null | undefined;

/** One of the two appearances a colour can be asked for. */
export type Appearance = "light" | "dark";

/** What a probe inherits where its own colour does not resolve: no real colour is this one. */
const UNRESOLVED = "rgb(1, 2, 4)";

/** A colour the page draws from its stylesheet (`var(--tint-accent-fg)`, a `color-mix()`), as the
 * `#rrggbb` it has now, for the places that cannot take CSS: an SVG drawn into a picture for the
 * native menu, xterm's cell decorations. The stylesheet stays the only place the colour is written
 * down; this reads it back, in the theme the page is in, or in `appearance` when that is given (the
 * native menu follows the appearance the app sets, which `prefers-color-scheme` reports). `appearance` holds for a token HeroUI
 * defines per appearance (`--accent-soft-foreground`); a `--tint-*` is declared once on `:root` and
 * computed there, so it follows the page's theme whatever is asked. The answer is kept per theme,
 * appearance and expression, so a caller may ask on every frame. `undefined` where the page cannot
 * resolve it: an expression that does not parse or names a variable that is not defined, no
 * stylesheet, no canvas. */
export function resolveCssColour(expression: string, appearance?: Appearance): string | undefined {
  const root = document.documentElement;
  const key = `${root.className}|${root.dataset.theme ?? ""}|${appearance ?? ""}|${expression}`;
  if (resolved.has(key)) return resolved.get(key);
  // A colour that does not resolve falls back to the inherited one, so the host's is one nothing resolves to.
  const host = document.createElement("div");
  host.style.color = UNRESOLVED;
  if (appearance) {
    host.className = appearance;
    host.dataset.theme = appearance;
  }
  const probe = document.createElement("span");
  probe.style.color = expression;
  host.appendChild(probe);
  root.appendChild(host);
  const computed = getComputedStyle(probe).color;
  const inherited = getComputedStyle(host).color;
  host.remove();
  let colour: string | undefined;
  if (computed && computed !== inherited) {
    if (canvas === undefined) canvas = document.createElement("canvas").getContext("2d", { willReadFrequently: true });
    if (canvas) {
      // Painted rather than parsed: the computed value of a colour mixed in oklab is an `oklab()`.
      canvas.clearRect(0, 0, 1, 1);
      canvas.fillStyle = computed;
      canvas.fillRect(0, 0, 1, 1);
      const [r, g, b] = canvas.getImageData(0, 0, 1, 1).data;
      colour = `#${[r, g, b].map((channel) => channel.toString(16).padStart(2, "0")).join("")}`;
    }
  }
  resolved.set(key, colour);
  return colour;
}
