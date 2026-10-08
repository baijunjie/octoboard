import {
  Button,
  Chip,
  CloseButton,
  Popover,
  SearchField,
  Tag,
  TagGroup,
} from "@heroui/react";
import { BrushCleaning, Check, ListFilter } from "lucide-react";
import type { Key } from "react-aria";
import React, { type RefObject, useRef, useState } from "react";

import { handFocusOff } from "../components/handFocusOff";
import { PICKED_TAG_CLASS, PICKED_TAG_REMOVE_CLASS } from "../components/tagStyle";
import { TitledControl } from "../components/TitledControl";
import { usePointerFocusReturn } from "../components/usePointerFocusReturn";
import { useT } from "../i18n/react";
import { withoutTags, withTag } from "../projectFiltering";
import { keepFocus, RowIconButton } from "./rows";

/** A console's project filter: the keyword a project's name must contain and the tags it must all
 * carry. */
export interface ProjectFilter {
  keyword: string;
  tags: string[];
}

/** A change to a console's stored filter, applied to the filter as it is stored, not as it shows. */
export type FilterUpdate = (stored: ProjectFilter) => ProjectFilter;

export const NO_FILTER: ProjectFilter = { keyword: "", tags: [] };

/** Whether `filter` is narrowing the list at all. */
function isFiltering(filter: ProjectFilter): boolean {
  return filter.keyword.trim() !== "" || filter.tags.length > 0;
}

/** The filter button by the Projects heading: it opens a popover with a field whose keyword
 * filters the project list as it is typed, and the tags in use (`vocabulary`) to pick from; a
 * project must carry every one picked. Escape closes it and keeps the filter, and so does Enter in
 * the field (on a tag, Enter picks or drops it). While a filter is in force, a button beside it
 * clears the keyword and the tags together. `filter.tags` is already narrowed to the vocabulary,
 * so changes go up as updates to apply to the stored filter, which still holds the tags no project
 * carries. `holderRef` is the element around the filter button, for the chips on the heading to
 * hand focus to. */
export function ProjectFilterButton({
  filter,
  vocabulary,
  onChange,
  holderRef,
}: {
  filter: ProjectFilter;
  vocabulary: string[];
  onChange: (update: FilterUpdate) => void;
  holderRef: RefObject<HTMLDivElement | null>;
}): React.ReactElement {
  const t = useT();
  const [open, setOpen] = useState(false);
  // The button never takes focus on a press, and react-aria hands focus back to it when the popover
  // closes, which would pull it off the terminal; the element the press came from gets it back.
  const pointerFocus = usePointerFocusReturn();
  const buttonHolder = holderRef;
  const clearHolder = useRef<HTMLSpanElement>(null);

  const onOpenChange = (isOpen: boolean) => {
    setOpen(isOpen);
    const restore = pointerFocus.opened(isOpen);
    if (isOpen || !restore) return;
    // react-aria hands focus back to the button a moment after the popover closes; it is passed on
    // the moment it arrives, to where it came from, or dropped when nothing had it (no terminal
    // yet) rather than left on the button showing a ring nobody asked for.
    const holder = buttonHolder.current;
    if (!holder) return;
    const passOn = () => {
      if (restore.previous?.isConnected) restore.previous.focus();
      else (document.activeElement as HTMLElement | null)?.blur();
    };
    if (holder.contains(document.activeElement)) {
      passOn();
      return;
    }
    const listeners = new AbortController();
    holder.addEventListener("focusin", passOn, {
      once: true,
      signal: listeners.signal,
    });
    setTimeout(() => listeners.abort(), 500);
  };

  const clear = () => {
    // The clear button goes away with the filter, taking keyboard focus with it.
    handFocusOff(clearHolder.current, buttonHolder.current, true);
    onChange(() => NO_FILTER);
  };

  // No wrapper of its own: `SectionHeading` already lays its action controls out in a row and
  // spaces them, so these two sit directly among the buttons that follow them.
  return (
    <>
      {isFiltering(filter) && (
        <span ref={clearHolder} className="flex">
          <RowIconButton
            icon={BrushCleaning}
            label={t("sidebar.filter.clear")}
            onPress={clear}
          />
        </span>
      )}
      <div
        ref={buttonHolder}
        className="flex"
        onPointerDownCapture={pointerFocus.onPointerDownCapture}
      >
        <Popover isOpen={open} onOpenChange={onOpenChange}>
          <TitledControl title={t("sidebar.filter.open")}>
            <Button
              isIconOnly
              size="sm"
              variant="ghost"
              aria-label={t("sidebar.filter.open")}
              preventFocusOnPress
              className="size-6 min-w-0 rounded-md text-muted hover:text-foreground"
            >
              <ListFilter aria-hidden="true" className="size-4" />
            </Button>
          </TitledControl>
          <Popover.Content placement="bottom end" className="w-64">
            <Popover.Dialog aria-label={t("sidebar.filter.open")}>
              {/* The field clears itself on Escape; here Escape closes the popover and keeps the filter. */}
              <div
                className="flex flex-col gap-3"
                onKeyDownCapture={(e) => {
                  if (e.key !== "Escape") return;
                  e.preventDefault();
                  e.stopPropagation();
                  onOpenChange(false);
                }}
              >
                <SearchField
                  aria-label={t("sidebar.filter.field")}
                  value={filter.keyword}
                  onChange={(keyword) => onChange((f) => ({ ...f, keyword }))}
                  onSubmit={() => onOpenChange(false)}
                  autoFocus
                  fullWidth
                  variant="secondary"
                >
                  <SearchField.Group>
                    <SearchField.SearchIcon />
                    <SearchField.Input
                      placeholder={t("sidebar.filter.placeholder")}
                    />
                    <SearchField.ClearButton />
                  </SearchField.Group>
                </SearchField>
                <TagPicker
                  vocabulary={vocabulary}
                  selected={filter.tags}
                  onChange={(added, removed) =>
                    onChange((f) => ({
                      ...f,
                      tags: added.reduce(withTag, withoutTags(f.tags, removed)),
                    }))
                  }
                />
              </div>
            </Popover.Dialog>
          </Popover.Content>
        </Popover>
      </div>
    </>
  );
}

/** The tags in use, each a toggle: the tags picked are the ones a project must all carry. With
 * none in use, a line says where tags come from. */
function TagPicker({
  vocabulary,
  selected,
  onChange,
}: {
  vocabulary: string[];
  selected: string[];
  onChange: (added: string[], removed: string[]) => void;
}): React.ReactElement {
  const t = useT();
  if (vocabulary.length === 0)
    return <p className="text-xs text-muted">{t("sidebar.filter.noTags")}</p>;
  return (
    <TagGroup
      aria-label={t("sidebar.filter.tags")}
      size="sm"
      selectionMode="multiple"
      selectedKeys={selected}
      onSelectionChange={(keys) => {
        const next = vocabulary.filter(
          (tag) => keys === "all" || keys.has(tag),
        );
        onChange(
          next.filter((tag) => !selected.includes(tag)),
          selected.filter((tag) => !next.includes(tag)),
        );
      }}
    >
      <TagGroup.List className="scrollbar max-h-40 overflow-y-auto">
        {vocabulary.map((tag) => (
          <Tag key={tag} id={tag} textValue={tag}>
            {({ isSelected }) => (
              <>
                {/* Picked is shown by a glyph too, not by the fill alone. */}
                {isSelected && (
                  <Check aria-hidden="true" className="size-3 shrink-0" />
                )}
                <span dir="auto">{tag}</span>
              </>
            )}
          </Tag>
        ))}
      </TagGroup.List>
    </TagGroup>
  );
}

/** The keyword in force, beside the Projects heading, with a button that drops just the keyword
 * (the Clear filter button by the filter button clears it with the tags). The chip goes away with
 * the keyword, so a removal that leaves focus on its button moves focus to `returnFocusTo`, the
 * element around the filter button, rather than let it drop to `<body>`; the button's focus ring
 * follows whatever modality the interaction had already set, so after Tabbing into the chip a
 * click on its button also shows the ring, as the browser treats the scripted `focus()` as
 * focus-visible. HeroUI's `Chip` has no remove part
 * of its own, so this is `CloseButton`, the part `Tag.RemoveButton` is built on. Outside a
 * collection row nothing composes its name, so the label stands on its own. */
export function ProjectFilterTag({
  keyword,
  onRemove,
  returnFocusTo,
}: {
  keyword: string;
  onRemove: () => void;
  returnFocusTo: RefObject<HTMLElement | null>;
}): React.ReactElement {
  const t = useT();
  const chip = useRef<HTMLSpanElement>(null);
  const label = t("sidebar.filter.removeKeyword", { keyword });
  const remove = () => {
    handFocusOff(chip.current, returnFocusTo.current, false);
    onRemove();
  };
  return (
    <Chip
      ref={chip}
      size="sm"
      variant="soft"
      className="min-w-0 max-w-full"
    >
      <Chip.Label dir="auto" className="truncate">
        {keyword}
      </Chip.Label>
      <TitledControl title={label}>
        <CloseButton
          aria-label={label}
          preventFocusOnPress
          onPress={remove}
          // The hover fill is a tint of the foreground, not HeroUI's `bg-default-hover`, which is a
          // translucent fill about as strong as the chip's own (`.sidebar-fills`) and so adds little on it.
          // `bg-transparent` cancels `.close-button--default`'s own fill, so the resting look is the chip's.
          className="touch-target size-3 shrink-0 rounded-full bg-transparent hover:bg-foreground/10 hover:text-foreground [&_svg]:size-[inherit]"
        />
      </TitledControl>
    </Chip>
  );
}

/** The tags picked in the filter, on the Projects heading right after the keyword and wrapping
 * onto further lines when they do not fit, each removable on its own. They are filled with the
 * soft accent colour, which sets them apart from the keyword chip. The heading's Clear filter button
 * clears them with the keyword. The group goes away with its last tag, so a removal that empties
 * it while it holds focus moves focus to `returnFocusTo`, the element around the filter button;
 * the tag unmounting would drop it to `<body>`, and the terminal would stop receiving keystrokes.
 * The button's focus ring follows whatever modality the interaction had already set, so it shows
 * for a removal from the keyboard. One edge: after Tabbing into the group, a click on a tag's
 * remove button also shows the ring, as the browser treats the scripted `focus()` as
 * focus-visible. */
export function ProjectFilterTags({
  tags,
  onRemove,
  returnFocusTo,
}: {
  tags: string[];
  onRemove: (tags: string[]) => void;
  returnFocusTo: RefObject<HTMLElement | null>;
}): React.ReactElement {
  const t = useT();
  const group = useRef<HTMLDivElement>(null);
  const remove = (keys: Set<Key>) => {
    const removed = tags.filter((tag) => keys.has(tag));
    // The group only goes away with its last tag.
    const emptied = removed.length === tags.length;
    handFocusOff(emptied ? group.current : null, returnFocusTo.current, false);
    onRemove(removed);
  };
  return (
    <TagGroup
      ref={group}
      aria-label={t("sidebar.filter.picked")}
      size="sm"
      // The group and its list take no box of their own, so each tag is an item of the heading's
      // wrapping row and flows on right after the keyword.
      className="contents"
      // Clicking a tag must not take keyboard focus off the terminal; the remove button still gets its click.
      onMouseDownCapture={keepFocus}
      onRemove={remove}
    >
      <TagGroup.List className="contents">
        {tags.map((tag) => (
          <Tag
            key={tag}
            id={tag}
            textValue={tag}
            className={PICKED_TAG_CLASS}
          >
            {() => (
              <>
                <span dir="auto" className="truncate">
                  {tag}
                </span>
                <TitledControl title={t("sidebar.filter.removeTag", { tag })}>
                  <Tag.RemoveButton
                    aria-label={t("common.removeTag")}
                    preventFocusOnPress
                    className={PICKED_TAG_REMOVE_CLASS}
                  />
                </TitledControl>
              </>
            )}
          </Tag>
        ))}
      </TagGroup.List>
    </TagGroup>
  );
}
