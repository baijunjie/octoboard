# Keeping the UI responsive in WebKit

The rules are in `docs/conventions/ui-code.md`, "Performance in the app's WebKit", and
`docs/conventions/ui-components.md`, "Styling mechanisms". What they leave to judgement and measurement:

## Judge an interaction's cost as restyle cost times forced passes, and make the restyle cheap first

Each synchronous layout read after a DOM change runs the pending restyle there and then, so an interaction whose
components each change the DOM and then measure (a pane switch re-measuring its fades, scrollers and FLIP animations)
pays the full restyle once per read. Cheapening the restyle shrinks every pass at once. Once nothing of ours restyles
the document, React Aria's `inert` toggle on each modal overlay is what is left of a menu's open and close; the only
levers then are fewer elements under `#root` and cheaper style recalculation per element.

## Count the built rules a CSS change leaves with a bare rightmost compound

After adding code that emits CSS, count the built rules whose rightmost compound starts with `:where(`, `:is(`, `:not(`
or `*` against the count before: about 150 remain, and a jump of dozens is the thing to fix, since WebKit tries such a
rule against every element.

## Unwrapping a `:where()` changes what the rule beats

Moving a `:where()` off the rightmost compound gives the rule the weight of what was inside it: a tag-only compound
still loses to a utility class on the element, but one carrying a pseudo-class or an attribute now beats it, so check
what the rule is meant to lose to.

## Measure latency only in the system WKWebView

Playwright's headless WebKit reports style-recalculation cost several times too low and Chrome lower still, so a cause
that dominates in the app can look negligible in both: never conclude from either that styling is not the bottleneck.

- `pnpm bench:wkwebview` in `packages/ui` (base against branch, same machine, nothing else busy) is the measure;
  `packages/ui/scripts/wkbench/README.md`, "Running it" and "Reading the tables", has the options and columns.
- Its `--engine webkit|chromium` runs are only a coarse before-against-after ratio, and its Chrome CPU profiles
  (`--profile`) only say which code spends the time, never how long the app takes.
- Its scenarios rename sessions, create consoles and sessions and trust folders: leave `--port` out so it starts its own
  throwaway daemon, or pass it only for a daemon you started yourself on a scratch `HOME`.
