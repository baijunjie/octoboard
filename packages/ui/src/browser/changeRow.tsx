import React from "react";

import { FadeOverflow } from "../components/FadeOverflow";
import type { PlainMessageKey, Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { StatusChip } from "../viewer/StatusChip";
import { STATUS_MARKS, type StatusKey } from "../viewer/statusMarks";
import { displayWirePath, wireBaseName } from "../wirePath";
import type { ChangeItem } from "./changes";
import { RowActions } from "./RowActions";

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
 * the row sits under its folder in a tree (`inFolder`), the folder the file is in. `shown` is what
 * is painted, and `sentence` what goes into the row's accessible name: they differ for a bare
 * folder, which is isolated only in the sentence, since the element painting it isolates it already. */
function rowDetail(t: Translate, item: ChangeItem, inFolder: boolean): { shown: string; sentence: string } {
  const { entry } = item;
  if (entry.group !== "conflicted") {
    const { old: before, new: after } = entry;
    let prose: string | undefined;
    if (before.state === "out_of_scope") prose = t("git.row.fromOutside", { path: isolate(displayWirePath(before.repository_path)) });
    else if (after.state === "out_of_scope") prose = t("git.row.toOutside", { path: isolate(displayWirePath(after.repository_path)) });
    else if (before.state === "present" && after.state === "present" && before.path !== after.path) {
      prose = t("git.row.from", { path: isolate(displayWirePath(before.path)) });
    }
    if (prose !== undefined) return { shown: prose, sentence: prose };
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
      <RowActions path={item.path} name={name} />
    </>
  );
}
