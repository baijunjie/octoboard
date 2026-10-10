/**
 * The markup the rendering library (`@pierre/diffs`) draws, in one place: every element and attribute
 * the viewer reads or sets in its shadow root, for the code range and for the separators of
 * collapsed lines (`expansion.ts` holds the policy and calls this for each DOM read and write).
 * Only what `renderer.tsx` itself relies on (the drawn check and the library's stylesheet) is named
 * there. Kept out of that module because it is loaded lazily and this is needed at a key press. It
 * depends on the library's markup (a shadow root under the frame, with one `[data-content]` column
 * of code per file or diff side, and `[data-separator]` bars), so a library upgrade is checked
 * against it.
 */

/** What a control of a separator expands: lines above its gap's hunk below it, below the hunk above
 * it, both, or the whole file. */
export type ExpandKind = "up" | "down" | "both" | "all";

/** The attribute naming each kind of control, in the order Tab walks a gap's controls. */
const KIND_ATTRIBUTES: [ExpandKind, string][] = [
  ["up", "data-expand-up"],
  ["down", "data-expand-down"],
  ["both", "data-expand-both"],
  ["all", "data-expand-all-button"],
];

function kindOf(control: Element): ExpandKind | undefined {
  return KIND_ATTRIBUTES.find(([, attribute]) => control.hasAttribute(attribute))?.[0];
}

/** The library's shadow root under a rendered frame, when it has drawn there. */
function shadowRootOf(frame: HTMLElement): ShadowRoot | undefined {
  return Array.from(frame.querySelectorAll("*")).find((element) => element.shadowRoot)?.shadowRoot ?? undefined;
}

/**
 * The code a rendered frame shows, as a range from its first content column to its last, leaving
 * out the line numbers before them. `undefined` when the frame holds no rendered code (plain text,
 * or the renderer's own loading state); `null` when the library's host is there but has drawn no
 * code yet.
 */
export function renderedCodeRange(frame: HTMLElement): Range | null | undefined {
  const root = shadowRootOf(frame);
  if (!root) return undefined;
  const columns = root.querySelectorAll("[data-content]");
  if (columns.length === 0) return null;
  const last = columns[columns.length - 1];
  const range = frame.ownerDocument.createRange();
  range.setStart(columns[0], 0);
  range.setEnd(last, last.childNodes.length);
  return range;
}

/** The scrolling frame a library host sits in, which takes focus. */
function codeRegionOf(root: ShadowRoot): HTMLElement | null {
  return root.host.closest<HTMLElement>("[role=region]");
}

/** Gives focus to the code frame around `root`. */
export function focusCodeRegion(root: ShadowRoot, options?: FocusOptions): void {
  codeRegionOf(root)?.focus(options);
}

/** The number of the gap a separator, or a control or label inside one, belongs to: the run of lines
 * before the hunk with that number (the one after the last hunk is numbered past it). */
export function gapOf(element: Element): number | undefined {
  const index = element.closest<HTMLElement>("[data-separator][data-expand-index]")?.dataset.expandIndex;
  return index === undefined ? undefined : Number(index);
}

/** The separators drawn in `root`, one per gap and copy: the library draws a gap's separator in the
 * gutter and again in the code column, and a split diff has a bar on each side. */
export function separatorsOf(root: ParentNode): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>("[data-separator]"));
}

/** Whether a separator is the copy in the code column, which is the one a reader's eye is on. */
export function inCodeColumn(separator: HTMLElement): boolean {
  return separator.closest("[data-content]") !== null;
}

/** The text of a separator saying how many lines it hides. */
export function labelOf(separator: HTMLElement): HTMLElement | null {
  return separator.querySelector<HTMLElement>("[data-unmodified-lines]");
}

/** How many lines the library says a separator hides, as digits ("" when it names no count). It is
 * read from the library's English text, once: what the library writes there again (a text that is
 * not the one `writeLabel` wrote) is read anew. */
export function hiddenCount(label: HTMLElement): string {
  if (label.textContent !== label.dataset.written) label.dataset.count = /\d+/.exec(label.textContent ?? "")?.[0] ?? "";
  return label.dataset.count ?? "";
}

/** Replaces a separator's text. Its direction is its own (in an Arabic app, the sentence is Arabic)
 * whatever the code frame's. A tooltip repeats it, for a label cut short by the room beside the
 * whole-file control. */
export function writeLabel(label: HTMLElement, text: string): void {
  label.dir = "auto";
  label.title = text;
  label.textContent = text;
  label.dataset.written = text;
}

/** A separator's controls with what each expands, apart from the library's own "expand all", which
 * it draws only for a long gap, never shows, and expands that gap alone with. */
export function controlsOf(separator: HTMLElement): { control: HTMLElement; kind: ExpandKind }[] {
  const found: { control: HTMLElement; kind: ExpandKind }[] = [];
  for (const control of separator.querySelectorAll<HTMLElement>("[data-expand-button]")) {
    const kind = kindOf(control);
    if (kind && (kind !== "all" || control.hasAttribute("data-whole-file"))) found.push({ control, kind });
  }
  return found;
}

/** Offers the whole file from a separator, at the end of its bar, or takes the offer away. */
export function setWholeFile(separator: HTMLElement, offered: boolean): void {
  const existing = separator.querySelector("[data-whole-file]");
  if (!offered) {
    existing?.remove();
    return;
  }
  if (existing) return;
  // Built by hand, and not a HeroUI `Button`: the library draws these bars inside its own shadow
  // root, where no React tree reaches; it is named and made a tab stop in `describeControl`.
  const all = separator.ownerDocument.createElement("div");
  all.setAttribute("role", "button");
  all.setAttribute("data-expand-button", "");
  all.setAttribute("data-expand-all-button", "");
  all.setAttribute("data-whole-file", "");
  separator.append(all);
}

/**
 * Names a control and makes it reachable. Of the copies the library draws of a separator only the
 * `first` of each control is a tab stop and is named; the rest are hidden from assistive technology.
 * A control `disabled` says it is not available for now.
 */
export function describeControl(control: HTMLElement, kind: ExpandKind, name: string, first: boolean, disabled: boolean): void {
  if (kind === "all") control.textContent = name;
  control.setAttribute("tabindex", first ? "0" : "-1");
  if (!first) {
    control.setAttribute("aria-hidden", "true");
    return;
  }
  control.setAttribute("aria-label", name);
  // A native tooltip, and not `TitledControl`'s HeroUI one, for the same reason as the hand-built
  // control (`setWholeFile`): there is no React tree here to render a tooltip into.
  control.setAttribute("title", name);
  control.removeAttribute("aria-hidden");
  if (disabled) control.setAttribute("aria-disabled", "true");
  else control.removeAttribute("aria-disabled");
}

/** The event's target when it is a control of a separator. */
export function controlOf(event: Event): HTMLElement | undefined {
  return event.target instanceof HTMLElement && event.target.hasAttribute("data-expand-button") ? event.target : undefined;
}

/** The gap and control an event landed on, when it landed on an expandable separator: the label
 * counts as its first button. */
export function expandTargetOf(event: Event): { gap: number; kind: ExpandKind } | undefined {
  for (const node of event.composedPath()) {
    if (!(node instanceof HTMLElement)) continue;
    const isLabel = node.hasAttribute("data-separator-content") || node.hasAttribute("data-unmodified-lines");
    if (!isLabel && !node.hasAttribute("data-expand-button")) continue;
    const gap = gapOf(node);
    if (gap === undefined) return undefined;
    const kind = isLabel
      ? Array.from(node.closest("[data-separator]")?.querySelectorAll("[data-expand-button]") ?? [])
          .map(kindOf)
          .find((found) => found !== undefined && found !== "all")
      : kindOf(node);
    return kind ? { gap, kind } : undefined;
  }
  return undefined;
}

/** The controls Tab stops at (the first copy of each is made a tab stop), gap by gap from the top. */
function separatorTabStops(root: ParentNode): HTMLElement[] {
  const rank = (control: HTMLElement) => KIND_ATTRIBUTES.findIndex(([, attribute]) => control.hasAttribute(attribute));
  const gap = (control: HTMLElement) => gapOf(control) ?? Number.POSITIVE_INFINITY;
  return Array.from(root.querySelectorAll<HTMLElement>("[data-expand-button][tabindex='0']")).sort((a, b) => gap(a) - gap(b) || rank(a) - rank(b));
}

/** Focuses a control with the focus ring a keyboard move draws: the ring is on the control's own
 * attribute, as the library's shadow root is out of reach of the app's focus handling. */
function focusControl(control: HTMLElement): void {
  control.setAttribute("data-focus-visible", "");
  control.focus();
}

/** Takes the ring off a control that lost focus. */
export function dropFocusRing(control: Element): void {
  control.removeAttribute("data-focus-visible");
}

/** Moves focus into a rendered diff's first control for expanding its collapsed lines (Tab from the
 * frame, which the library's shadow root keeps out of the dialog's own Tab order). Returns whether
 * there was one. */
export function focusFirstSeparatorControl(frame: HTMLElement): boolean {
  const root = shadowRootOf(frame);
  const control = root ? separatorTabStops(root)[0] : undefined;
  if (!control) return false;
  focusControl(control);
  return true;
}

/**
 * Moves focus one control along from `from`, the way Tab does through the separators of `root`: the
 * dialog's focus containment walks the page's own tree and does not see into a shadow root, so Tab
 * is moved along the controls by hand. Forwards from the last control nothing is done, so the
 * dialog goes on to what follows the code; backwards from the first it goes to the code frame,
 * which `focusFirstSeparatorControl` enters again. Returns whether it moved focus.
 */
export function moveAlongControls(root: ShadowRoot, from: HTMLElement, backwards: boolean): boolean {
  const stops = separatorTabStops(root);
  const next = stops[stops.indexOf(from) + (backwards ? -1 : 1)];
  if (next) {
    focusControl(next);
    return true;
  }
  if (!backwards || !codeRegionOf(root)) return false;
  focusCodeRegion(root);
  return true;
}

/** Puts focus back on a control after a redraw replaced the one that had it: on the same control if
 * there is still one, else the nearest one's. Returns whether focus is on a control now (it already
 * was, or there was one to give it to). */
export function refocusControl(root: ShadowRoot, wanted: { gap: number; kind: ExpandKind }): boolean {
  const active = root.activeElement;
  if (active?.isConnected && active.hasAttribute("data-expand-button")) return true;
  const controls = separatorTabStops(root);
  const sameGap = controls.filter((control) => gapOf(control) === wanted.gap);
  const target =
    sameGap.find((control) => kindOf(control) === wanted.kind) ??
    sameGap[0] ??
    controls.find((control) => (gapOf(control) ?? Number.NEGATIVE_INFINITY) > wanted.gap) ??
    controls[controls.length - 1];
  if (!target) return false;
  focusControl(target);
  return true;
}
