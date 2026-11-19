import React, { useEffect, useRef, useState } from "react";

import { ResizeSeparator } from "../../../components/panel/ResizeSeparator";
import { useT } from "../../../i18n/react";
import { createPersistedPreference } from "../../../preferences/persistedPreference";
import { PREFERENCE_KEYS } from "../../../preferences/preferenceKeys";

/** The least height the list and the preview each keep as the divider between them moves. */
export const MIN_SPLIT_PART = 80;

/** The list's even share, the default and what a double-click on the divider puts back. */
const EVEN = 0.5;

/** The share of the height the result list takes above the preview, kept across searches and restarts in this
 * browser. Anything stored that is not a share strictly between 0 and 1 reads as the even split. */
export const contentSearchSplit = createPersistedPreference<number>(
  PREFERENCE_KEYS.contentSearchSplit,
  (raw) => {
    const share = raw === null ? Number.NaN : Number(raw);
    return share > 0 && share < 1 ? share : EVEN;
  },
  (share) => (share === EVEN ? null : String(share)),
);

/**
 * The result list above the preview, with the divider between them that gives one the other's room: dragged, or from
 * the keyboard (`ResizeSeparator`: the arrows, Home and End; a double-click puts the even split back). Each keeps at
 * least `MIN_SPLIT_PART` pixels. The split is remembered as a share of the height (`contentSearchSplit`), so it holds
 * as the modal is resized. Without a preview the list takes the whole height.
 */
export function ContentSearchSplit({ list, preview }: { list: React.ReactNode; preview: React.ReactNode | null }): React.ReactElement {
  const t = useT();
  const stored = contentSearchSplit.useValue();
  // The share a drag in progress has reached, committed to the preference once the drag ends.
  const [dragged, setDragged] = useState<number>();
  const pending = useRef<number | undefined>(undefined);
  const ref = useRef<HTMLDivElement>(null);
  const [height, setHeight] = useState(0);
  // Measured after every render and whenever the box changes size, written only when it moved.
  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    const measure = () => {
      const next = element.clientHeight;
      setHeight((current) => (current === next ? current : next));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  });
  if (preview === null) return <div className="flex min-h-0 flex-1 flex-col">{list}</div>;
  const share = dragged ?? stored;
  const max = Math.max(MIN_SPLIT_PART, height - MIN_SPLIT_PART);
  const listHeight = Math.min(max, Math.max(MIN_SPLIT_PART, share * height));
  const commit = (next: number) => {
    pending.current = undefined;
    setDragged(undefined);
    contentSearchSplit.set(next);
  };
  return (
    <div ref={ref} className="flex min-h-0 flex-1 flex-col">
      <div data-content-search-list="" className="flex min-h-0 shrink-0 flex-col" style={{ height: `clamp(${MIN_SPLIT_PART}px, ${share * 100}%, calc(100% - ${MIN_SPLIT_PART}px))` }}>
        {list}
      </div>
      <div className="relative h-0 shrink-0">
        <ResizeSeparator
          orientation="horizontal"
          value={Math.round(listHeight)}
          min={MIN_SPLIT_PART}
          max={max}
          label={t("contentSearch.resize")}
          grow={1}
          appearance="grip"
          tooltip={t("pane.resize")}
          onChange={(value, options) => {
            if (height <= 0) return;
            const next = value / height;
            if (options?.commit === false) {
              pending.current = next;
              setDragged(next);
            } else {
              commit(next);
            }
          }}
          onCommit={() => {
            if (pending.current !== undefined) commit(pending.current);
          }}
          onReset={() => commit(EVEN)}
          className="absolute inset-x-0 -top-3 z-10 h-6"
        />
      </div>
      {preview}
    </div>
  );
}
