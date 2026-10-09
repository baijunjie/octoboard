import { Chip, Header, ListBox } from "@heroui/react";
import React, { useMemo } from "react";
import { Collection, ListLayout, Virtualizer, type Key } from "react-aria-components";

import { FadeOverflow } from "../components/FadeOverflow";
import type { PlainMessageKey } from "../i18n/catalog";
import { useCurrentLanguage, useT } from "../i18n/react";
import { displayWirePath, wireBaseName } from "../wirePath";
import { SECTION_LABELS, sections, type ChangeItem, type ChangeSection, type ChangeStatus } from "./changes";

/** Every row is one line of this height, and every section heading one of its own, which is what
 * lets the list be virtualized. */
export const CHANGE_ROW_HEIGHT = 28;
const HEADING_HEIGHT = 30;

/** Each status's letter and its colour; the row's name says it in words. A, M, D, R and T are
 * `git status --short`'s; there an untracked file is `??` and a conflict `U`, here they are U
 * (untracked) and C (conflicted), one letter each like the rest. */
const STATUS_MARKS: Record<ChangeStatus | "untracked", { letter: string; color: "success" | "danger" | "warning" | "accent" }> = {
  added: { letter: "A", color: "success" },
  untracked: { letter: "U", color: "success" },
  deleted: { letter: "D", color: "danger" },
  modified: { letter: "M", color: "warning" },
  renamed: { letter: "R", color: "accent" },
  typeChanged: { letter: "T", color: "accent" },
  conflicted: { letter: "C", color: "danger" },
};

const STATUS_WORDS: Record<ChangeStatus | "untracked", PlainMessageKey> = {
  added: "git.status.added",
  untracked: "git.status.untracked",
  deleted: "git.status.deleted",
  modified: "git.status.modified",
  renamed: "git.status.renamed",
  typeChanged: "git.status.typeChanged",
  conflicted: "git.status.conflicted",
};

function statusOf(item: ChangeItem): ChangeStatus | "untracked" {
  return item.section === "untracked" ? "untracked" : item.status;
}

/** What a row says beside its file's name: where a rename came from or went, and otherwise the
 * folder the file is in. */
function rowDetail(t: ReturnType<typeof useT>, item: ChangeItem): string {
  const { entry } = item;
  if (entry.group !== "conflicted") {
    const { old: before, new: after } = entry;
    if (before.state === "out_of_scope") return t("git.row.fromOutside", { path: displayWirePath(before.repository_path) });
    if (after.state === "out_of_scope") return t("git.row.toOutside", { path: displayWirePath(after.repository_path) });
    if (before.state === "present" && after.state === "present" && before.path !== after.path) {
      return t("git.row.from", { path: displayWirePath(before.path) });
    }
  }
  const slash = item.path.lastIndexOf("/");
  return slash === -1 ? "" : displayWirePath(item.path.slice(0, slash));
}

/** How far below the top of the list's content the row of `key` is, or nothing when the list has
 * no such row. Rows and headings are each one height and laid out one after another below the
 * list's top padding (`py-1`), so where a row is follows from what comes before it. */
export function changeRowOffset(items: readonly ChangeItem[], key: string): number | undefined {
  let top = 4;
  for (const group of sections(items)) {
    top += HEADING_HEIGHT;
    const index = group.items.findIndex((item) => item.key === key);
    if (index !== -1) return top + index * CHANGE_ROW_HEIGHT;
    top += group.items.length * CHANGE_ROW_HEIGHT;
  }
  return undefined;
}

/**
 * A worktree's changes as a list in sections — staged, in conflict, unstaged, untracked — each
 * headed with its count: HeroUI's `ListBox`, virtualized. A row's action is to open its change in
 * the viewer; nothing is selectable in the list's sense. The change the viewer showed last is
 * tinted and named as selected in its row's label instead, as the file tree marks its file.
 */
export function ChangeList({
  items,
  selected,
  label,
  onOpen,
  listRef,
}: {
  items: ChangeItem[];
  /** The key of the change the viewer showed last. */
  selected: string | undefined;
  label: string;
  onOpen: (item: ChangeItem) => void;
  listRef: React.Ref<HTMLDivElement>;
}): React.ReactElement {
  const language = useCurrentLanguage();
  // A section's `id` is what the collection keys it by; a change's own `key` keys its row.
  const groups = useMemo(() => sections(items).map((group) => ({ id: group.section, ...group })), [items]);
  const byKey = useMemo(() => new Map(items.map((item) => [item.key, item])), [items]);
  const onAction = (key: Key) => {
    const item = byKey.get(String(key));
    if (item) onOpen(item);
  };
  return (
    <Virtualizer layout={ListLayout} layoutOptions={{ rowHeight: CHANGE_ROW_HEIGHT, headingHeight: HEADING_HEIGHT }}>
      <ListBox
        ref={listRef}
        aria-label={label}
        items={groups}
        onAction={onAction}
        dependencies={[language, selected]}
        className="min-h-0 flex-1 overflow-auto px-1 py-1 outline-none"
      >
        {(group) => <ChangeSectionView key={group.section} section={group.section} items={group.items} selected={selected} />}
      </ListBox>
    </Virtualizer>
  );
}

function ChangeSectionView({
  section,
  items,
  selected,
}: {
  section: ChangeSection;
  items: ChangeItem[];
  selected: string | undefined;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  return (
    <ListBox.Section id={section}>
      <Header className="flex h-[30px] items-center gap-2 px-2 text-xs font-medium text-muted">
        {t(SECTION_LABELS[section])}
        <span>{new Intl.NumberFormat(language).format(items.length)}</span>
      </Header>
      <Collection items={items} dependencies={[language, selected]}>
        {(item) => <ChangeRow item={item} isSelected={item.key === selected} />}
      </Collection>
    </ListBox.Section>
  );
}

function ChangeRow({ item, isSelected }: { item: ChangeItem; isSelected: boolean }): React.ReactElement {
  const t = useT();
  const name = wireBaseName(item.path);
  const status = statusOf(item);
  const mark = STATUS_MARKS[status];
  const detail = rowDetail(t, item);
  const word = t(STATUS_WORDS[status]);
  const ariaLabel =
    detail === ""
      ? t(isSelected ? "git.row.selected" : "git.row", { name, status: word })
      : t(isSelected ? "git.row.detail.selected" : "git.row.detail", { name, status: word, detail });
  // Marked `data-current`, not `data-selected`: react-aria-components writes the row's
  // `data-selected` itself, from a selection the list does not have.
  return (
    <ListBox.Item
      id={item.key}
      textValue={name}
      aria-label={ariaLabel}
      data-current={isSelected || undefined}
      className="h-7 min-h-0 gap-2 rounded-md px-2 py-0 text-sm data-[current]:bg-panel-selected"
    >
      <Chip aria-hidden="true" size="sm" variant="soft" color={mark.color} className="w-5 shrink-0 justify-center px-0 font-mono">
        {mark.letter}
      </Chip>
      <FadeOverflow as="span" dir="auto" className="min-w-0 shrink" titleWhenClipped={name}>
        {name}
      </FadeOverflow>
      {detail !== "" && (
        <FadeOverflow as="span" dir="auto" className="min-w-0 flex-1 text-xs text-muted" titleWhenClipped={detail}>
          {detail}
        </FadeOverflow>
      )}
    </ListBox.Item>
  );
}
