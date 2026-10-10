import React from "react";

import { FadeOverflow } from "../components/FadeOverflow";
import type { PlainMessageKey, Translate } from "../i18n/catalog";
import { useCurrentLanguage, useT } from "../i18n/react";
import { StatusChip } from "../viewer/StatusChip";
import { STATUS_MARKS, type StatusKey } from "../viewer/statusMarks";
import { displayWirePath, wireBaseName } from "../wirePath";
import { SECTION_LABELS, type ChangeItem, type ChangeSection } from "./changes";
import { ChangeRowActions } from "./ChangeRowActions";

/** Every section heading is one line of this height, which, with the rows' (`ROW_HEIGHT`), is what
 * lets the list be virtualized, flat or as a tree. */
export const HEADING_HEIGHT = 30;
/** A section's heading, in both layouts. `h-[30px]` is `HEADING_HEIGHT`. */
export const HEADING_CLASS = "flex h-[30px] items-center gap-2 px-3 text-xs font-medium text-muted";

const STATUS_WORDS: Record<StatusKey, PlainMessageKey> = {
  added: "git.status.added",
  untracked: "git.status.untracked",
  deleted: "git.status.deleted",
  modified: "git.status.modified",
  renamed: "git.status.renamed",
  typeChanged: "git.status.typeChanged",
  conflicted: "git.status.conflicted",
};

/** What a row says beside its file's name: where a rename came from or went, and otherwise, unless
 * the row sits under its folder in a tree (`inFolder`), the folder the file is in. */
function rowDetail(t: Translate, item: ChangeItem, inFolder: boolean): string {
  const { entry } = item;
  if (entry.group !== "conflicted") {
    const { old: before, new: after } = entry;
    if (before.state === "out_of_scope") return t("git.row.fromOutside", { path: displayWirePath(before.repository_path) });
    if (after.state === "out_of_scope") return t("git.row.toOutside", { path: displayWirePath(after.repository_path) });
    if (before.state === "present" && after.state === "present" && before.path !== after.path) {
      return t("git.row.from", { path: displayWirePath(before.path) });
    }
  }
  if (inFolder) return "";
  const slash = item.path.lastIndexOf("/");
  return slash === -1 ? "" : displayWirePath(item.path.slice(0, slash));
}

/** What a change's row is named, for assistive technology, and says beside its name. */
export function changeRowText(
  t: Translate,
  item: ChangeItem,
  isSelected: boolean,
  inFolder: boolean,
): { name: string; detail: string; ariaLabel: string } {
  const name = wireBaseName(item.path);
  const detail = rowDetail(t, item, inFolder);
  const status = t(STATUS_WORDS[item.status]);
  const ariaLabel =
    detail === ""
      ? t(isSelected ? "git.row.selected" : "git.row", { name, status })
      : t(isSelected ? "git.row.detail.selected" : "git.row.detail", { name, status, detail });
  return { name, detail, ariaLabel };
}

/** What a change's row shows: its status mark, its file's name and the detail beside it, and at its
 * end the button of the row's actions. */
export function ChangeRowBody({ item, name, detail }: { item: ChangeItem; name: string; detail: string }): React.ReactElement {
  return (
    <>
      <StatusChip aria-hidden="true" status={item.status} className="w-5 shrink-0 justify-center px-0 font-mono">
        {STATUS_MARKS[item.status].letter}
      </StatusChip>
      <FadeOverflow as="span" dir="auto" className="min-w-0 shrink" titleWhenClipped={name}>
        {name}
      </FadeOverflow>
      {detail !== "" && (
        <FadeOverflow as="span" dir="auto" className="min-w-0 flex-1 text-xs text-muted" titleWhenClipped={detail}>
          {detail}
        </FadeOverflow>
      )}
      <ChangeRowActions item={item} name={name} />
    </>
  );
}

/** What a section's heading says: the section's name and how many changes it holds. */
export function SectionHeadingText({ section, count }: { section: ChangeSection; count: number }): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  return (
    <>
      {t(SECTION_LABELS[section])}
      <span>{new Intl.NumberFormat(language).format(count)}</span>
    </>
  );
}
