import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { useFocusHandoff } from "../../components/focus/useFocusHandoff";
import { useLatest } from "../../components/hooks/useLatest";
import { isImeKey } from "../../input/imeKey";
import { isPrimaryModifier } from "../../input/keys";
import type { HostFacts } from "../../platform/host";
import { useHost } from "../../platform/react";
import { DEFAULT_FIND_OPTIONS, findPattern, MAX_FIND_MATCHES, stepMatch, type FindOptions } from "./find";
import type { FindBar, FindStatus } from "./FindBar";
import { clearMatches, findMatches, findRegions, hasPlaceholder, paintMatches, revealMatch } from "./findDom";
import { findSelection } from "./findSelection";
import { shadowRootsIn } from "../code/rendererDom";
import { setKeyboardModality } from "../../primitives";

/** How long the query rests before the search runs, so typing in a large file is not a search per
 * key; and how long a burst of changes to the drawn text rests before the search runs again. */
const SETTLE_MS = 150;

interface FindResult {
  ranges: Range[];
  capped: boolean;
  invalid: boolean;
  /** The text surfaces found, or -1 before the first search. */
  regions: number;
  /** What it was searched for: it counts only while that is still what the bar asks. */
  query: string;
  options: FindOptions;
  subjectKey: string;
  /** The reader asked for a match to be shown, and this is the search that answers it. */
  reveal: boolean;
}

const NOT_SEARCHED: FindResult = { ranges: [], capped: false, invalid: false, regions: -1, query: "", options: DEFAULT_FIND_OPTIONS, subjectKey: "", reveal: false };

const NO_RANGES: Range[] = [];

/** Whether a press is Find: the physical F key with ⌘ on an Apple keyboard, Ctrl elsewhere. It is the
 * shortcut list's `FIND_BINDING` (`shortcuts/commands.ts`), not read from it, as the viewer does not
 * import `shortcuts/`; the viewer's tests press the key from that entry, so they fail if the two part. */
function isFind(event: Pick<KeyboardEvent, "code" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">, host: Pick<HostFacts, "modifier">): boolean {
  return event.code === "KeyF" && !event.altKey && !event.shiftKey && isPrimaryModifier(event, host);
}

/** The gap each region leaves for the open bar, in pixels: `--find-bar-gap` (`style.css`), which is
 * the height of its `::before`, read where the content inherits it. */
function barGapOf(element: Element): number {
  const gap = getComputedStyle(element).getPropertyValue("--find-bar-gap").trim();
  const size = parseFloat(gap) || 0;
  return gap.endsWith("rem") ? size * (parseFloat(getComputedStyle(document.documentElement).fontSize) || 16) : size;
}

/** Whether the content holds a surface to search, followed as surfaces come and go. They are
 * elements of the viewer's own, outside the renderer's shadow roots, so their coming and going is
 * seen without observing the drawn text. */
function useSearchable(contentRef: React.RefObject<HTMLElement | null>): boolean {
  const [searchable, setSearchable] = useState(false);
  useEffect(() => {
    const container = contentRef.current;
    if (!container) return;
    const update = () => setSearchable(findRegions(container).length > 0);
    update();
    const observer = new MutationObserver(update);
    observer.observe(container, { childList: true, subtree: true });
    return () => observer.disconnect();
  }, []);
  return searchable;
}

/** A number that goes up when the drawn text may have changed, once the changes rest, while `watch`
 * is set: the mutations of the content and of each shadow root under it. */
function useDrawnTextVersion(contentRef: React.RefObject<HTMLElement | null>, watch: boolean): [number, () => void] {
  const [version, setVersion] = useState(0);
  useEffect(() => {
    const container = contentRef.current;
    if (!watch || !container) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const observer = new MutationObserver(() => {
      clearTimeout(timer);
      timer = setTimeout(() => setVersion((v) => v + 1), SETTLE_MS);
    });
    const init = { childList: true, subtree: true, characterData: true };
    observer.observe(container, init);
    for (const root of shadowRootsIn(container)) observer.observe(root, init);
    return () => {
      clearTimeout(timer);
      observer.disconnect();
    };
  }, [watch, version]);
  return [version, () => setVersion((v) => v + 1)];
}

/**
 * Find in file for the viewer: the bar's state, the search over the text its surfaces draw
 * (`findRegions`), and the highlights.
 *
 * ⌘F inside the viewer opens the bar and focuses its query (selecting it when the bar is open
 * already), where there is text to search. Escape closes the bar first, handing focus back to the
 * code, and only the next Escape reaches the viewer's own close (the dialog does not close on
 * Escape while the bar is up: `keyboardDismissHeld`); a control with its popup open keeps the key,
 * and a tooltip that is open is dismissed by that press, as anywhere else. Both keys are handled by a
 * listener on the dialog element, before React's handlers, as Left and Right are
 * (`useNavigationFocus`); the capture-phase listeners on `document` (react-aria's tooltip and its
 * tracking of keyboard input) still see them. The header's toggle closes the bar too, and takes focus
 * back; a bar that goes with the text it searched hands its focus to the code (`useFocusHandoff`).
 *
 * The bar and its query outlive the subject: moving to another file searches it for the same
 * query, once it is drawn. The search runs again whenever what is drawn changes (a redraw for
 * wrapping or the layout, the file read again, collapsed lines expanded), which a mutation observer
 * on the content and on each shadow root under it reports; the match the reader is on stays the
 * same number. A result answers only the query, options and file it was searched for: until the
 * search for a change lands the bar shows no count and paints no match, and the match to scroll to
 * is the one of the search that answers the change, not of the result it replaces. The matches are
 * painted as CSS highlights, which change neither the DOM nor the selection; where the browser has
 * none, the bar still counts and scrolls. While the bar is open each searched scroller leaves a gap
 * at its top for it (`style.css`), and a scroller that is scrolled is moved by that gap as the bar
 * opens and closes, so its text does not jump.
 *
 * `waiting` is the content still on its way, which keeps the bar up while nothing is drawn yet; the
 * bar is otherwise there only while the viewer shows text to search.
 */
export function useFind({
  headerRef,
  contentRef,
  subjectKey,
  waiting,
}: {
  headerRef: React.RefObject<HTMLElement | null>;
  contentRef: React.RefObject<HTMLElement | null>;
  subjectKey: string;
  waiting: boolean;
}): {
  /** The bar's props while it is on screen. */
  bar: React.ComponentProps<typeof FindBar> | null;
  /** The header's find toggle, while the viewer shows text to search. */
  toggle: { open: boolean; onToggle: () => void; ref: React.RefObject<HTMLButtonElement | null> } | null;
} {
  const host = useHost();
  const [open, setOpen] = useState(false);
  const searchable = useSearchable(contentRef);
  const [query, setQuery] = useState("");
  const [settled, setSettled] = useState("");
  const [options, setOptions] = useState(DEFAULT_FIND_OPTIONS);
  const [version, searchAgain] = useDrawnTextVersion(contentRef, open);
  const [result, setResult] = useState(NOT_SEARCHED);
  // The match the reader is on, for the subject it was reached in.
  const [stepped, setStepped] = useState({ key: subjectKey, index: 0 });
  const inputRef = useRef<HTMLInputElement>(null);
  const barRef = useRef<HTMLDivElement>(null);
  const toggleRef = useRef<HTMLButtonElement>(null);
  // Whether the first match of the next search is to be scrolled into view: after the reader changed
  // what is searched for, not after a redraw. It is the search that answers, not whichever result
  // is on screen when the change is made, which is still the previous one's.
  const revealNext = useRef(false);
  // Whether the match the reader stepped to is to be scrolled into view.
  const revealStep = useRef(false);
  // The result whose reveal has been made.
  const revealed = useRef<FindResult | null>(null);

  const visible = open && (result.regions !== 0 || waiting);
  // The result counts while it answers what the bar asks; between a change of the query, the
  // options or the file and its search, the bar shows no count and paints no match.
  const ready = result.regions >= 0 && result.query === settled && settled === query && result.options === options && result.subjectKey === subjectKey;
  const ranges = ready ? result.ranges : NO_RANGES;
  const index = stepped.key === subjectKey ? Math.min(stepped.index, Math.max(ranges.length - 1, 0)) : 0;

  useEffect(() => {
    const timer = setTimeout(() => {
      setSettled(query);
      // A query typed back to the one searched has no search to answer it: search again.
      if (query === settled && revealNext.current) searchAgain();
    }, SETTLE_MS);
    return () => clearTimeout(timer);
  }, [query]);

  // A new subject is searched for the same query, and its first match shown.
  useEffect(() => {
    revealNext.current = true;
  }, [subjectKey]);

  useEffect(() => {
    const container = contentRef.current;
    if (!open || !container) return;
    const regions = findRegions(container);
    // Content still on its way, or a region still showing its placeholder, is not text to answer
    // with: the search waits for the drawn text (a redraw of it searches again) and keeps its reveal.
    if (waiting || hasPlaceholder(regions)) {
      setResult((current) => (current.regions === 0 ? NOT_SEARCHED : current));
      return;
    }
    const pattern = findPattern(settled, options);
    const found = pattern.kind === "pattern" ? findMatches(regions, pattern.pattern, MAX_FIND_MATCHES) : { ranges: [], capped: false };
    // A query still being typed is searched for its settled text, which does not answer the reveal.
    const reveal = revealNext.current && query === settled;
    if (reveal) revealNext.current = false;
    setResult({ ...found, invalid: pattern.kind === "invalid", regions: regions.length, query: settled, options, subjectKey, reveal });
  }, [open, settled, options, version, subjectKey, waiting]);

  useEffect(() => {
    if (!open || !visible || !ready) {
      clearMatches();
      return;
    }
    paintMatches(ranges, index);
    const range = ranges[index];
    if (result.reveal && revealed.current !== result) {
      revealed.current = result;
      if (range) revealMatch(range);
    } else if (revealStep.current) {
      revealStep.current = false;
      if (range) revealMatch(range);
    }
  }, [open, visible, ready, result, index, stepped]);
  useEffect(() => clearMatches, []);

  // The bar's gap pushes the scrolled text down as it opens and pulls it up as it closes, which a
  // browser without scroll anchoring (WebKit) shows as a jump: the scrollers that are scrolled
  // are moved by the gap, so the text stays where the reader had it.
  const gap = useRef({ shown: false, height: 0 });
  useLayoutEffect(() => {
    const container = contentRef.current;
    if (gap.current.shown === visible || !container) return;
    gap.current.shown = visible;
    // A browser with scroll anchoring keeps the text in place itself; correcting it too would move it twice.
    if (typeof CSS !== "undefined" && CSS.supports?.("overflow-anchor", "auto")) return;
    // The gap is the one the content inherits from the open bar's wrapper, so it is there to read
    // whether or not a region is drawn yet.
    if (visible) gap.current.height = barGapOf(container);
    for (const region of findRegions(container)) {
      if (region.scrollTop > 0) region.scrollTop = Math.max(0, region.scrollTop + (visible ? gap.current.height : -gap.current.height));
    }
  }, [visible]);

  const focusQuery = () => {
    inputRef.current?.focus();
    inputRef.current?.select();
  };
  const justOpened = useRef(false);
  useEffect(() => {
    if (open && justOpened.current && visible) {
      justOpened.current = false;
      focusQuery();
    }
  }, [open, visible, query]);

  /** Focus to the code, or the dialog where there is none left to take it. */
  const focusContent = () => {
    const container = contentRef.current;
    const target = (container && findRegions(container)[0]) ?? headerRef.current?.closest<HTMLElement>("[role=dialog]");
    target?.focus();
  };

  // The bar goes without `close` when the surface it searched does (an error, a disconnect, an
  // image): focus in it would fall to the page and take the dialog's keys with it.
  const handoff = useFocusHandoff(focusContent);

  /** Closes the bar. Focus in it goes to the code, as Escape and Close ask, or to the header's toggle
   * when that is what closed it, which is pressed without taking focus. */
  const close = (focusTo: "content" | "toggle" = "content") => {
    setOpen(false);
    setResult(NOT_SEARCHED);
    revealNext.current = false;
    revealStep.current = false;
    if (barRef.current?.contains(document.activeElement)) {
      if (focusTo === "toggle") toggleRef.current?.focus();
      else focusContent();
    }
  };

  const openBar = () => {
    justOpened.current = true;
    revealNext.current = true;
    setOpen(true);
  };

  // The latest of everything the listeners, added once, read.
  const latest = useLatest({ open: visible, close, subjectKey });
  useEffect(() => {
    const dialog = headerRef.current?.closest<HTMLElement>("[role=dialog]");
    if (!dialog) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (isImeKey(event)) return;
      if (isFind(event, host)) {
        const container = contentRef.current;
        // With the bar up already the key is the bar's, though nothing is drawn to search yet.
        if (!latest.current.open && (!container || findRegions(container).length === 0)) return;
        event.preventDefault();
        event.stopPropagation();
        setKeyboardModality();
        // Match pasting into the single-line query: its value cannot contain CR or LF.
        const selected = container && !barRef.current?.contains(document.activeElement) ? findSelection(container).replace(/[\r\n]/g, "") : "";
        if (selected) {
          setQuery(selected);
          setSettled(selected);
          setStepped({ key: latest.current.subjectKey, index: 0 });
          revealNext.current = true;
          justOpened.current = true;
          searchAgain();
        }
        if (latest.current.open) focusQuery();
        else openBar();
        return;
      }
      if (event.key !== "Escape" || !latest.current.open || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
      // A control with its popup open owns the key, as it does anywhere else in the dialog. The find
      // toggle is expanded while the bar is open and holds no popup.
      const target = event.target instanceof Element ? event.target : undefined;
      const expanded = target?.closest('[aria-expanded="true"]');
      if (expanded && expanded !== toggleRef.current && !barRef.current?.contains(expanded)) return;
      event.preventDefault();
      event.stopPropagation();
      latest.current.close();
    };
    dialog.addEventListener("keydown", onKeyDown);
    return () => dialog.removeEventListener("keydown", onKeyDown);
  }, [host]);

  const toggle = searchable || visible ? { open: visible, onToggle: () => (visible ? close("toggle") : openBar()), ref: toggleRef } : null;
  if (!visible) return { bar: null, toggle };
  const status: FindStatus =
    !ready || settled === "" ? { kind: "idle" } : result.invalid ? { kind: "invalid" } : ranges.length === 0 ? { kind: "none" } : { kind: "found", current: index + 1, total: ranges.length, capped: result.capped };
  const bar: React.ComponentProps<typeof FindBar> = {
    query,
    options,
    status,
    inputRef,
    barRef,
    focusHandoff: handoff,
    onQueryChange: (next: string) => {
      revealNext.current = true;
      setQuery(next);
      setStepped({ key: subjectKey, index: 0 });
    },
    onOptionsChange: (next: FindOptions) => {
      revealNext.current = true;
      setOptions(next);
      setStepped({ key: subjectKey, index: 0 });
    },
    onStep: (direction) => {
      // Enter before the query has rested searches it now, and shows its first match.
      if (query !== settled) {
        revealNext.current = true;
        setSettled(query);
        return;
      }
      if (!ready) return;
      revealStep.current = true;
      setStepped({ key: subjectKey, index: stepMatch(index, ranges.length, direction) });
    },
    onClose: () => close(),
  };
  return { bar, toggle };
}
