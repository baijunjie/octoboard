// Expanding the collapsed lines of a diff. The rendering library (`@pierre/diffs`) draws a separator
// where unmodified lines were collapsed and can expand it once it is given both sides' whole text;
// this module takes over what it leaves to a caller: reading the text on the first expansion, how
// much each expansion reveals, the separators' wording in the app's language, and keyboard access
// to them. Everything it reads or sets in the library's markup goes through `rendererDom.ts`. No
// library type crosses it.
import { bodiesPlan } from "./budgets";
import { ChangeBodiesError, type ChangeBodies } from "./content";
import {
  controlOf,
  controlsOf,
  describeControl,
  dropFocusRing,
  expandTargetOf,
  focusCodeRegion,
  hiddenCount,
  inCodeColumn,
  gapOf,
  labelOf,
  moveAlongControls,
  refocusControl,
  separatorsOf,
  setWholeFile,
  writeLabel,
  type ExpandKind,
} from "./rendererDom";

/** How many lines one expansion reveals. */
export const EXPANSION_STEP = 20;
/** The expansion of a gap that reveals all of what is left of it, counting from the first. */
const REST_AT = 3;

/** What the separators say, in the app's language. */
export interface SeparatorLabels {
  unmodified: (count: number) => string;
  unknown: string;
  above: string;
  below: string;
  between: string;
  all: string;
  loading: string;
  failedHere: string;
}

/** Where the expansion stands, as the caller words it beside the code. */
export type ExpansionStatus = { kind: "idle" } | { kind: "loading" } | { kind: "failed"; message: string } | { kind: "changed" };

/** What of the library's diff the expansion drives: a gap is the run of lines before hunk `gap`
 * (the one after the last hunk is numbered past it), `up` revealing lines from the hunk above it and
 * `down` from the one below. */
export interface HunkExpander {
  expandHunk: (gap: number, direction: "up" | "down" | "both", lines: number) => void;
}

/** What the expansion asks of the renderer's component. */
export interface ExpansionHooks {
  load: () => Promise<ChangeBodies>;
  labels: () => SeparatorLabels;
  report: (status: ExpansionStatus) => void;
  /** Shows the whole file, with no separator left. */
  showAll: () => void;
  /** Offers no expansion from here on: the separators go back to showing only what they hide. */
  stop: () => void;
}

/** A side's text as the library is handed it. */
interface LoadedFile {
  name: string;
  contents: string;
}

/** Any input ends a wish to have focus given back to a separator's control, as the user has moved on. */
const attached = new Set<Expansion>();
const onInput = () => attached.forEach((expansion) => expansion.forgetFocus());

const BOUND = new WeakMap<ShadowRoot, { current?: Expansion }>();

/** Listens on the library's shadow root once, for whichever expansion is current. Its listeners run
 * before the library's own, which would expand by its own step: a press here is ours alone. */
function bind(root: ShadowRoot): { current?: Expansion } {
  const bound = BOUND.get(root);
  if (bound) return bound;
  const holder: { current?: Expansion } = {};
  BOUND.set(root, holder);
  // A control never takes focus from a press, so a mouse press leaves where focus is as it was
  // (the library redraws the separators, and a focused one would be gone).
  root.addEventListener("mousedown", (event) => expandTargetOf(event) && event.preventDefault(), true);
  root.addEventListener(
    "click",
    (event) => {
      const target = expandTargetOf(event);
      if (!target) return;
      event.stopPropagation();
      holder.current?.activate(target.gap, target.kind, false);
    },
    true,
  );
  root.addEventListener(
    "keydown",
    (event) => {
      const { key, shiftKey } = event as KeyboardEvent;
      const control = controlOf(event);
      if (!control) return;
      if (key === "Tab") {
        if (!moveAlongControls(root, control, shiftKey)) return;
        event.preventDefault();
        event.stopPropagation();
        return;
      }
      const target = key === "Enter" || key === " " ? expandTargetOf(event) : undefined;
      if (!target) return;
      event.preventDefault();
      event.stopPropagation();
      holder.current?.activate(target.gap, target.kind, true);
    },
    true,
  );
  root.addEventListener("focusout", (event) => event.target instanceof HTMLElement && dropFocusRing(event.target));
  return holder;
}

/**
 * The expansion of one diff's separators, for as long as that diff is shown. The first expansion
 * reads both sides' text (`hooks.load`) and the library expands from it; each expansion of a gap
 * reveals `EXPANSION_STEP` lines, and the `REST_AT`th reveals the rest of that gap. A separator also
 * offers the whole file at once.
 *
 * It never lets the library read the text itself: the library is handed the text only once it is
 * here, so a failed read leaves the diff as it was, to be asked again from the same separator.
 */
export class Expansion {
  private bodies?: ChangeBodies;
  private loadingGap?: number;
  private failedGap?: number;
  private readonly counts = new Map<number, number>();
  private expander?: HunkExpander;
  private root?: ShadowRoot;
  private wantsFocus?: { gap: number; kind: ExpandKind };
  private ended = false;

  constructor(private readonly hooks: ExpansionHooks) {}

  /** What the library calls to read the sides' text: only ever after `activate` has them. */
  readonly files = (diff: { name: string; prevName?: string }): Promise<{ oldFile: LoadedFile; newFile: LoadedFile }> => {
    const { bodies } = this;
    if (!bodies) return Promise.reject(new Error("The bodies are read before the library asks for them"));
    return Promise.resolve({
      oldFile: { name: diff.prevName ?? diff.name, contents: bodies.old },
      newFile: { name: diff.name, contents: bodies.new },
    });
  };

  /** Called after each drawing of the diff: puts the separators in the app's wording and makes them
   * reachable, and gives focus back to a control the drawing replaced. */
  attach(host: HTMLElement, expander: HunkExpander): void {
    const root = host.shadowRoot;
    if (!root) return;
    this.expander = expander;
    this.ended = false;
    this.root = root;
    if (attached.size === 0) {
      root.ownerDocument.addEventListener("keydown", onInput, true);
      root.ownerDocument.addEventListener("pointerdown", onInput, true);
    }
    attached.add(this);
    bind(root).current = this;
    this.decorate();
    this.restoreFocus();
  }

  /** Called when the diff is gone. */
  detach(): void {
    this.ended = true;
    const root = this.root;
    if (!root) return;
    const holder = BOUND.get(root);
    if (holder?.current === this) holder.current = undefined;
    attached.delete(this);
    if (attached.size === 0) {
      root.ownerDocument.removeEventListener("keydown", onInput, true);
      root.ownerDocument.removeEventListener("pointerdown", onInput, true);
    }
    this.root = undefined;
  }

  forgetFocus(): void {
    this.wantsFocus = undefined;
  }

  /** Called when the change was read again: a failure of an earlier read of the bodies is not said
   * of this one, though the patch, and so this expansion, is the same. */
  forgetFailure(): void {
    if (this.failedGap === undefined) return;
    this.failedGap = undefined;
    this.decorate();
  }

  activate(gap: number, kind: ExpandKind, keyboard: boolean): void {
    if (this.loadingGap !== undefined) return;
    this.wantsFocus = keyboard ? { gap, kind } : undefined;
    if (this.bodies) {
      this.expand(gap, kind);
      return;
    }
    this.loadingGap = gap;
    this.failedGap = undefined;
    this.decorate();
    this.hooks.report({ kind: "loading" });
    this.hooks.load().then(
      (bodies) => {
        this.loadingGap = undefined;
        if (this.ended) return;
        // Past the render budget the lines stay collapsed, as when the daemon refuses the bodies.
        if (bodiesPlan(bodies) === "none") {
          this.hooks.stop();
          this.hooks.report({ kind: "idle" });
          return;
        }
        this.bodies = bodies;
        this.decorate();
        this.hooks.report({ kind: "idle" });
        this.expand(gap, kind);
      },
      (error: unknown) => {
        this.loadingGap = undefined;
        if (this.ended) return;
        const reason = error instanceof ChangeBodiesError ? error.reason : "failed";
        if (reason === "unavailable" || reason === "changed") {
          this.hooks.stop();
          this.hooks.report(reason === "changed" ? { kind: "changed" } : { kind: "idle" });
        } else if (reason === "failed") {
          this.failedGap = gap;
          this.decorate();
          this.hooks.report({ kind: "failed", message: error instanceof Error ? error.message : String(error) });
        } else {
          this.decorate();
          this.hooks.report({ kind: "idle" });
        }
      },
    );
  }

  private expand(gap: number, kind: ExpandKind): void {
    if (kind === "all") {
      this.hooks.showAll();
      return;
    }
    const count = (this.counts.get(gap) ?? 0) + 1;
    this.counts.set(gap, count);
    if (count >= REST_AT) this.expander?.expandHunk(gap, "both", Number.POSITIVE_INFINITY);
    else this.expander?.expandHunk(gap, kind, EXPANSION_STEP);
  }

  /** Words the separators and makes their controls reachable. Of the copies the library draws of a
   * separator (each gap's, in the gutter and in the code column) the gutter's shows the label and the
   * controls and the code column's shows only its whole-file control (its `[data-separator-wrapper]`
   * is hidden), and only the first of each control is a tab stop and is named. While a read is out every control says it is unavailable, as `activate`
   * lets none through until it is answered. */
  decorate(): void {
    const root = this.root;
    if (!root) return;
    const labels = this.hooks.labels();
    const seen = new Set<string>();
    const wholeFile = new Set<number>();
    for (const separator of separatorsOf(root)) {
      const gap = gapOf(separator);
      const label = labelOf(separator);
      if (label) {
        const count = hiddenCount(label);
        writeLabel(
          label,
          gap !== undefined && gap === this.loadingGap
            ? labels.loading
            : gap !== undefined && gap === this.failedGap
              ? labels.failedHere
              : count
                ? labels.unmodified(Number(count))
                : labels.unknown,
        );
      }
      if (gap === undefined) continue;
      // The library never shows its "expand all" and expands that gap alone; ours is drawn for every
      // gap, once (a split diff has a bar on each side), in the code column's copy because that
      // copy is the one on screen.
      const offered = inCodeColumn(separator) && !wholeFile.has(gap);
      if (offered) wholeFile.add(gap);
      setWholeFile(separator, offered);
      for (const { control, kind } of controlsOf(separator)) {
        const id = `${gap}:${kind}`;
        const first = !seen.has(id);
        seen.add(id);
        const name = labels[kind === "all" ? "all" : kind === "up" ? "above" : kind === "down" ? "below" : "between"];
        describeControl(control, kind, name, first, this.loadingGap !== undefined);
      }
    }
  }

  /** The drawing after an expansion replaces the separators, taking focus with them; it goes to the
   * same control if there is still one, else to the nearest separator's, else to the code. */
  private restoreFocus(): void {
    const wanted = this.wantsFocus;
    const root = this.root;
    if (!wanted || !root || refocusControl(root, wanted)) return;
    this.wantsFocus = undefined;
    focusCodeRegion(root, { preventScroll: true });
  }
}
