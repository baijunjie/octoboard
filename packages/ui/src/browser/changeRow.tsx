import React, { useRef, useState } from "react";

import { InlineChip } from "../components/InlineChip";
import { AnchoredTooltip } from "../components/AnchoredTooltip";
import { FadeOverflow } from "../components/FadeOverflow";
import type { PlainMessageKey, Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import { StatusChip } from "../viewer/StatusChip";
import { STATUS_LABELS, STATUS_MARKS, type StatusKey } from "../viewer/statusMarks";
import { displayWirePath, wireBaseName } from "../wirePath";
import type { ChangeItem } from "./changes";
import { RowActions } from "./RowActions";

// The lowercase words the row's accessible name is built from; the tooltip says the capitalised
// `STATUS_LABELS` instead, which is what the viewer's header chip shows.
const STATUS_WORDS: Record<StatusKey, PlainMessageKey> = {
  added: "git.status.added",
  untracked: "git.status.untracked",
  deleted: "git.status.deleted",
  modified: "git.status.modified",
  renamed: "git.status.renamed",
  typeChanged: "git.status.typeChanged",
  conflicted: "git.status.conflicted",
};

/** Rename and cross-project paths belong to prose; bare folder metadata does not. */
function pathDetail(item: ChangeItem): { id: "git.row.fromOutside" | "git.row.toOutside" | "git.row.from"; path: string } | undefined {
  const { entry } = item;
  if (entry.group === "conflicted") return undefined;
  const { old: before, new: after } = entry;
  if (before.state === "out_of_scope") return { id: "git.row.fromOutside", path: displayWirePath(before.repository_path) };
  if (after.state === "out_of_scope") return { id: "git.row.toOutside", path: displayWirePath(after.repository_path) };
  if (before.state === "present" && after.state === "present" && before.path !== after.path) return { id: "git.row.from", path: displayWirePath(before.path) };
  return undefined;
}

/** What a row says beside its file's name: where a rename came from or went, and otherwise, unless
 * the row sits under its folder in a tree (`inFolder`), the folder the file is in. `shown` is what
 * is painted, and `sentence` what goes into the row's accessible name: they differ for a bare
 * folder, which is isolated only in the sentence, since the element painting it isolates it already. */
function rowDetail(t: Translate, item: ChangeItem, inFolder: boolean): { shown: string; sentence: string } {
  const prose = pathDetail(item);
  if (prose) {
    const sentence = t(prose.id, { path: isolate(prose.path) });
    return { shown: sentence, sentence };
  }
  const slash = item.path.lastIndexOf("/");
  if (inFolder || slash === -1) return { shown: "", sentence: "" };
  const folder = displayWirePath(item.path.slice(0, slash));
  return { shown: folder, sentence: isolate(folder) };
}

/** What a change's row is named, for assistive technology, and says beside its name. */
export function changeRowText(
  t: Translate,
  item: ChangeItem,
  isSelected: boolean,
  inFolder: boolean,
): { name: string; detail: string; ariaLabel: string } {
  const name = wireBaseName(item.path);
  const { shown: detail, sentence } = rowDetail(t, item, inFolder);
  const status = t(STATUS_WORDS[item.status]);
  const ariaLabel =
    detail === ""
      ? t(isSelected ? "git.row.selected" : "git.row", { name: isolate(name), status })
      : t(isSelected ? "git.row.detail.selected" : "git.row.detail", { name: isolate(name), status, detail: sentence });
  return { name, detail, ariaLabel };
}

/** What a change's row shows: its status mark, its file's name and the detail beside it, and at its
 * end the button of the row's actions. */
export function ChangeRowBody({ item, name, detail, contextTargetRef }: { item: ChangeItem; name: string; detail: string; contextTargetRef: React.RefObject<HTMLElement | null> }): React.ReactElement {
  const prose = pathDetail(item);
  const shownDetail = prose ? <Message id={prose.id} params={{ path: <InlineChip monospace size="inherit">{prose.path}</InlineChip> }} /> : detail;
  return (
    <>
      <ChangeStatusChip status={item.status} />
      <FadeOverflow as="span" dir="auto" className="min-w-0 shrink" titleWhenClipped={name}>
        {name}
      </FadeOverflow>
      {detail !== "" && (
        <FadeOverflow as="span" dir="auto" className="min-w-0 flex-1 text-xs text-app-muted" titleWhenClipped={shownDetail}>
          {shownDetail}
        </FadeOverflow>
      )}
      <RowActions contextTargetRef={contextTargetRef} path={item.path} name={name} />
    </>
  );
}

/** The row's status letter, which says in a tooltip what it stands for. The row's accessible name
 * already carries the status, so the chip stays out of the accessibility tree. It takes the
 * anchored shape, not `IndicatorTooltip`: a row of a react-aria collection walks every focusable
 * descendant with the arrow keys, and an indicator's trigger would be a new stop in that walk
 * that opens its tooltip on focus. The chip reports the hover itself and is what the tooltip is
 * placed against, so nothing else in the row opens it. */
function ChangeStatusChip({ status }: { status: StatusKey }): React.ReactElement {
  const t = useT();
  const chip = useRef<HTMLSpanElement>(null);
  const [hovered, setHovered] = useState(false);
  return (
    <>
      <StatusChip
        ref={chip}
        aria-hidden="true"
        status={status}
        className="w-5 shrink-0 justify-center px-0 font-mono"
        onPointerEnter={() => setHovered(true)}
        onPointerLeave={() => setHovered(false)}
      >
        {STATUS_MARKS[status].letter}
      </StatusChip>
      <AnchoredTooltip tooltip={t(STATUS_LABELS[status])} anchor={chip} hovered={hovered} />
    </>
  );
}
