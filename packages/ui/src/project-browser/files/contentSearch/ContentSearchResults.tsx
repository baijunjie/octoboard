import React, { useEffect, useState } from "react";

import { useScrollFade } from "../../../components/scroll/scrollFade";
import { FadeOverflow } from "../../../components/text/FadeOverflow";
import { displayWirePath, wireBaseName } from "../../../format/wirePath";
import { useCurrentLanguage, useT } from "../../../i18n/react";
import { ListBox, ListBoxItem } from "../../../primitives";
import type { ContentMatch } from "../../../protocol";
import { matchFolder, matchKey, matchSegments } from "./matchText";

/** The height of a result row, every row the same, for the list to be virtualized: a search can find 20,000 lines. */
export const RESULT_ROW_HEIGHT = 32;

/**
 * The matching lines of a content search, one row each: the line's text with its matches marked by the daemon's ranges,
 * then the file's name and the line number, and with `showFolder` the file's folder after them, for a modal that shows
 * no preview naming it: results in same-named files in different folders are told apart. Rows are appended as batches arrive; they are keyed by file and line, so
 * the selection and the scroll position stay where they are. One row is selected at a time (`selectedId`): the arrow
 * keys and a click move it, Enter and a double click open it (`onOpen`).
 */
export function ContentSearchResults({ matches, showFolder, selectedId, onSelectedChange, onOpen, listRef }: {
  matches: readonly ContentMatch[];
  showFolder: boolean;
  selectedId: string | null;
  onSelectedChange: (id: string) => void;
  onOpen: (match: ContentMatch) => void;
  listRef: React.Ref<HTMLDivElement>;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const fade = useScrollFade({ ref: listRef, className: "min-h-0 flex-1 overflow-y-auto overflow-x-hidden py-1 outline-none" });
  const byKey = new Map(matches.map((match) => [matchKey(match), match]));
  return (
    <ListBox
      {...fade}
      label={t("contentSearch.results")}
      data-content-search-results=""
      items={matches}
      // A row's accessible name is worded in the current language, which react-aria's cached rows would keep.
      dependencies={[language, showFolder]}
      rowHeight={RESULT_ROW_HEIGHT}
      selectedId={selectedId}
      onSelectedChange={onSelectedChange}
      onAction={(id) => {
        const match = byKey.get(id);
        if (match) onOpen(match);
      }}
    >
      {(match: ContentMatch) => <ResultRow match={match} showFolder={showFolder} />}
    </ListBox>
  );
}

/** The width of one cell (a character) of the fixed-width font `font` (a computed `font`), measured once per font. */
const charWidths = new Map<string, number>();
function charWidth(font: string): number | undefined {
  const known = charWidths.get(font);
  if (known !== undefined) return known;
  const context = document.createElement("canvas").getContext("2d");
  if (!context) return undefined;
  context.font = font;
  const width = context.measureText("0".repeat(16)).width / 16;
  if (!(width > 0)) return undefined;
  charWidths.set(font, width);
  return width;
}

/** How many cells of the fixed-width font the row's text has room for (see `matchSegments`), measured from the element that holds it and kept up
 * as it changes size; unknown until it has been laid out. The text's box takes the room the row leaves it whatever it
 * holds, so the text cut to fit does not change it. */
function useFit(): [(element: HTMLSpanElement | null) => void, number | undefined] {
  // A state rather than a ref: a collection mounts its rows' content after the row itself, so the element is watched
  // once it is attached.
  const [element, setElement] = useState<HTMLSpanElement | null>(null);
  const [fit, setFit] = useState<number>();
  useEffect(() => {
    if (!element) return;
    const measure = () => {
      const width = charWidth(getComputedStyle(element).font);
      const next = width === undefined || element.clientWidth <= 0 ? undefined : Math.floor(element.clientWidth / width);
      setFit((current) => (current === next ? current : next));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [element]);
  return [setElement, fit];
}

function ResultRow({ match, showFolder }: { match: ContentMatch; showFolder: boolean }): React.ReactElement {
  const t = useT();
  const [textRef, fit] = useFit();
  const { segments, clippedStart, clippedEnd } = matchSegments(match, fit);
  const name = wireBaseName(match.path);
  const folder = showFolder ? matchFolder(match.path) : "";
  // The name reads the line as the daemon sent it, not as cut to the row's width.
  const text = matchSegments(match).segments.map((segment) => segment.text).join("");
  return (
    <ListBoxItem
      id={matchKey(match)}
      textValue={text}
      label={t("contentSearch.result", { text, path: displayWirePath(match.path), line: match.line })}
      className="mx-(--row-fill-inset) w-auto gap-3 rounded-md px-2 hover:bg-panel-hover ui-selected:bg-panel-selected ui-selected:hover:bg-panel-selected"
      data-content-search-result={matchKey(match)}
    >
      {/* Code reads left to right whatever the interface's direction, and keeps its spaces. A line of the file is not a
          label: it is cut before its match with an ellipsis rather than faded, so the match stays in view. */}
      <span ref={textRef} dir="ltr" className="min-w-0 flex-1 truncate font-mono text-xs whitespace-pre">
        {clippedStart && "…"}
        {segments.map((segment, i) =>
          segment.marked ? (
            // The current find match's look: a fill, a colour of its own and an underline, so a match is told from
            // the rest by more than colour.
            <mark key={i} className="rounded-xs bg-(--find-current-bg) text-(--find-current-fg) underline">{segment.text}</mark>
          ) : (
            <React.Fragment key={i}>{segment.text}</React.Fragment>
          ),
        )}
        {clippedEnd && "…"}
      </span>
      {/* The folder gives way before the name. */}
      <span dir="ltr" className={`flex ${folder !== "" ? "max-w-[50%]" : "max-w-[40%]"} shrink-0 items-baseline gap-2 text-xs text-app-muted`}>
        <span className="flex min-w-0 shrink">
          <FadeOverflow as="span" dir="ltr" className="min-w-0" titleWhenClipped={name}>{name}</FadeOverflow>
          <span className="shrink-0 tabular-nums">:{match.line}</span>
        </span>
        {folder !== "" && (
          <FadeOverflow as="span" dir="ltr" className="min-w-0 shrink-[100]" titleWhenClipped={folder}>{folder}</FadeOverflow>
        )}
      </span>
    </ListBoxItem>
  );
}
