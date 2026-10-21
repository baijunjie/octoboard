import { FileSymlink, FolderGit2 } from "lucide-react";
import React, { useRef, useState } from "react";

import { InlineChip } from "../components/InlineChip";
import { AnchoredTooltip } from "../components/AnchoredTooltip";
import { FadeOverflow } from "../components/FadeOverflow";
import { FileTypeIcon } from "../components/FileTypeIcon";
import type { PlainMessageKey, Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { LineStats } from "../protocol";
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

/** What a change adds and removes, when it is worth saying: the daemon counted it and it is not a
 * change of nothing but a mode or a name. */
function lineStats(item: ChangeItem): LineStats | undefined {
  const { entry } = item;
  if (entry.group === "conflicted" || !entry.stats) return undefined;
  return entry.stats.added + entry.stats.removed > 0 ? entry.stats : undefined;
}

/** What a change's row is named, for assistive technology, and says beside its name. The line
 * counts the row shows at its end are part of the sentence after the name and status, with the
 * detail. */
export function changeRowText(
  t: Translate,
  item: ChangeItem,
  isSelected: boolean,
  inFolder: boolean,
): { name: string; detail: string; ariaLabel: string } {
  const name = wireBaseName(item.path);
  const { shown: detail, sentence: detailSentence } = rowDetail(t, item, inFolder);
  const status = t(STATUS_WORDS[item.status]);
  const stats = lineStats(item);
  const lines = stats ? t("git.row.lines", stats) : "";
  const sentence = detailSentence !== "" && lines !== "" ? t("git.row.detailLines", { detail: detailSentence, lines }) : detailSentence || lines;
  const ariaLabel =
    sentence === ""
      ? t(isSelected ? "git.row.selected" : "git.row", { name: isolate(name), status })
      : t(isSelected ? "git.row.detail.selected" : "git.row.detail", { name: isolate(name), status, detail: sentence });
  return { name, detail, ariaLabel };
}

/** What a change's row shows: its icon, its status mark, its file's name and the detail beside it,
 * and at its end the lines it adds and removes and the button of the row's actions. The button
 * keeps its room while hidden, so the counts do not move when it shows. Host actions use the live
 * directory of the list even when the change itself names versions in commits. */
export function ChangeRowBody({ item, name, detail, contextTargetRef }: { item: ChangeItem; name: string; detail: string; contextTargetRef: React.RefObject<HTMLElement | null> }): React.ReactElement {
  const prose = pathDetail(item);
  const shownDetail = prose ? <Message id={prose.id} params={{ path: <InlineChip monospace size="inherit">{prose.path}</InlineChip> }} /> : detail;
  return (
    <>
      <ChangeIcon item={item} />
      <ChangeStatusChip status={item.status} />
      <FadeOverflow as="span" dir="auto" className="min-w-0 shrink" titleWhenClipped={name}>
        {name}
      </FadeOverflow>
      {detail !== "" ? (
        <FadeOverflow as="span" dir="auto" className="min-w-0 flex-1 text-xs text-app-muted" titleWhenClipped={shownDetail}>
          {shownDetail}
        </FadeOverflow>
      ) : (
        <span className="flex-1" />
      )}
      <LineCounts item={item} />
      <RowActions reserve contextTargetRef={contextTargetRef} path={item.path} name={name} />
    </>
  );
}

/** The lines a change adds and removes, at the end of its row: `+N` in green and `-M` in red, each
 * left out when it is none. It is a visual duplicate of what the row's accessible name says
 * (`changeRowText`), so it stays out of the accessibility tree, and it keeps its order and its
 * digits whatever the text direction, as a diff's own gutter does. It never shrinks: the name gives
 * way first. */
function LineCounts({ item }: { item: ChangeItem }): React.ReactElement | null {
  const language = useCurrentLanguage();
  const stats = lineStats(item);
  if (!stats) return null;
  const number = new Intl.NumberFormat(language);
  return (
    <span aria-hidden="true" dir="ltr" className="flex shrink-0 gap-1.5 text-xs tabular-nums">
      {stats.added > 0 && <span className="text-line-added">+{number.format(stats.added)}</span>}
      {stats.removed > 0 && <span className="text-line-removed">-{number.format(stats.removed)}</span>}
    </span>
  );
}

/** The icon of what a change's path names, from the side the path is taken from: a link or a
 * submodule by its kind, and anything else, a path in conflict included, by its file type. */
function ChangeIcon({ item }: { item: ChangeItem }): React.ReactElement {
  const { entry } = item;
  const side = entry.group === "conflicted" ? undefined : entry.new.state === "present" ? entry.new : entry.old;
  const kind = side?.state === "present" ? side.kind : "file";
  if (kind === "file") return <FileTypeIcon path={item.path} />;
  const Icon = kind === "symlink" ? FileSymlink : FolderGit2;
  return <Icon aria-hidden="true" className="size-4 shrink-0 text-app-muted" />;
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
