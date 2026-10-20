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
  handFocusToCode,
  hiddenCount,
  inCodeColumn,
  gapOf,
  labelOf,
  moveAlongControls,
  refocusControl,
  removeSeparator,
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
  /** The label's name as a control. It holds the count the label shows, so the name contains the
   * visible text (a voice command saying that text finds the control). */
  hiddenAll: (count: number) => string;
  all: string;
  loading: string;
  failedHere: string;
}

/** Where the expansion stands, as the caller words it beside the code. */
export type ExpansionStatus =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "failed"; message: string }
  | { kind: "changed" }
  | { kind: "overBudget" }
  | { kind: "unavailable" };

/** What of the library's diff the expansion drives: the instance `onPostRender` hands over, of which
 * only `expandHunk` is used, so a library upgrade is checked against its gap numbering and directions.
 * A gap is the run of lines before hunk `gap` (the one after the last hunk is numbered past it), `up`
 * revealing lines from the hunk above it and `down` from the one below. */
export interface HunkExpander {
  expandHunk: (gap: number, direction: "up" | "down" | "both", lines: number) => void;
}

/** What the expansion asks of the renderer's component. */
export interface ExpansionHooks {
  load: () => Promise<ChangeBodies>;
  labels: () => SeparatorLabels;
  /** The run of unmodified lines after the last hunk when the patch's file length settles it: the
   * number of that hunk's gap (the one past the last hunk) and how many lines it holds. */
  trailing: () => { gap: number; lines: number } | undefined;
  report: (status: ExpansionStatus) => void;
  /** Shows the whole file, with no separator left. */
  showAll: () => void;
  /** Offers no expansion from here on: the separators go back to showing only what they hide. */
  stop: () => void;
  /** Told after separator wording changes so the renderer can draw its visible labels. */
  onLabels?: (root: ShadowRoot) => void;
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

/** Listens on the library's shadow root once, for whichever expansion is current. Registered for the
 * capture phase on the shadow root, so they run before the library's listeners on the elements inside
 * it, and `stopPropagation` keeps a handled press from reaching them (they would expand by the
 * library's own step); a library upgrade is checked for that. The library also redraws the separators
 * after each expansion, which `attach` relies on to decorate them again. */
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
      // A click on the control that has focus is to be handed focus back like a key press: that is
      // how a screen reader presses the control it is on (VoiceOver's press arrives as a click), and
      // the redraw takes that control away with the focus on it.
      const active = root.activeElement;
      holder.current?.activate(target.gap, target.kind, active !== null && event.composedPath().includes(active));
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
 * from one of its arrows reveals `EXPANSION_STEP` lines, and the `REST_AT`th reveals the rest of
 * that gap. A separator's text reveals the whole of its gap at once, and a separator also offers
 * the whole file at once.
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

  /** Expands `gap` by `kind`. `focused` says the control used had focus, which the drawing that
   * follows takes away with it, so focus is to be given back (`restoreFocus`). */
  activate(gap: number, kind: ExpandKind, focused: boolean): void {
    if (this.loadingGap !== undefined) return;
    this.wantsFocus = focused ? { gap, kind } : undefined;
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
        // Past the render budget the lines stay collapsed, and it is said.
        if (bodiesPlan(bodies) === "none") {
          this.hooks.stop();
          this.hooks.report({ kind: "overBudget" });
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
          this.hooks.report({ kind: reason });
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
    // The separator's own text reveals the whole of its gap at once, which is what its wording —
    // the number of lines it hides — reads as an offer of.
    if (kind === "gap") {
      this.counts.set(gap, REST_AT);
      this.expander?.expandHunk(gap, "both", Number.POSITIVE_INFINITY);
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
   * lets none through until it is answered. The bar after the last hunk says how many lines follow
   * when the file's length is known and the library has not yet counted it itself, and is taken away
   * when none do. */
  decorate(): void {
    const root = this.root;
    if (!root) return;
    const labels = this.hooks.labels();
    const trailing = this.hooks.trailing();
    const seen = new Set<string>();
    const wholeFile = new Set<number>();
    for (const separator of separatorsOf(root)) {
      const gap = gapOf(separator);
      const ends = trailing !== undefined && gap === trailing.gap;
      const label = labelOf(separator);
      // The library words what it knows: before the bodies are handed over it cannot count the run
      // after the last hunk and draws no number, and from then on it names what is still hidden,
      // which `trailing.lines` (the run as the daemon counted it, whole) no longer is once part of
      // it is revealed. So the library's number wins whenever it names one, and the daemon's is
      // used only where it does not; the two never both apply.
      const count = label ? hiddenCount(label) : undefined;
      // The file ends with the last hunk: the bar for what follows it has nothing to offer. A bar
      // the library drew with a count has lines to reveal, whatever the daemon counted.
      if (ends && !count && trailing.lines === 0) {
        removeSeparator(separator);
        continue;
      }
      const text =
        gap !== undefined && gap === this.loadingGap
          ? labels.loading
          : gap !== undefined && gap === this.failedGap
            ? labels.failedHere
            : count
              ? labels.unmodified(Number(count))
              : ends
                ? labels.unmodified(trailing.lines)
                : labels.unknown;
      if (label) writeLabel(label, text);
      // How many lines the label's control would reveal, where either side counted them.
      const revealed = count ? Number(count) : ends ? trailing.lines : undefined;
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
        // The label's control is named for its action and its tooltip says the visible sentence,
        // whatever state that is in (counted, loading, failed...): the tooltip also gives back what
        // the label's available room cuts off.
        const name =
          kind === "gap"
            ? revealed !== undefined
              ? labels.hiddenAll(revealed)
              : labels.between
            : labels[kind === "all" ? "all" : kind === "up" ? "above" : kind === "down" ? "below" : "between"];
        describeControl(control, kind, name, kind === "gap" ? text : name, first, this.loadingGap !== undefined);
      }
    }
    this.hooks.onLabels?.(root);
  }

  /** The drawing after an expansion replaces the separators, taking focus with them; it goes to the
   * same control if there is still one, else to the nearest separator's, else to the code. */
  private restoreFocus(): void {
    const wanted = this.wantsFocus;
    const root = this.root;
    if (!wanted || !root || refocusControl(root, wanted)) return;
    this.wantsFocus = undefined;
    handFocusToCode(root);
  }
}
