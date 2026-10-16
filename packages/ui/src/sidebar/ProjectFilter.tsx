import {
  Button,
  Popover,
  SearchField,
  Tag,
  TagGroup,
} from "@heroui/react";
import { Check, Funnel, FunnelX, Tags } from "lucide-react";
import React, { useEffect, useRef, useState } from "react";

import { InlineChip } from "../components/InlineChip";
import { EmptyPanel } from "../components/EmptyPanel";
import { FadeOverflow } from "../components/FadeOverflow";
import { TitledControl } from "../components/TitledControl";
import { usePointerFocusReturn } from "../components/usePointerFocusReturn";
import { useScrollFade } from "../components/scrollFade";
import { useTrimmedField } from "../dialogs/useTrimmedField";
import { useT } from "../i18n/react";
import { withoutTags, withTag } from "../projectFiltering";

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
export function isFiltering(filter: ProjectFilter): boolean {
  return filter.keyword !== "" || filter.tags.length > 0;
}

/** The filter button by the Projects heading: it opens a popover with a field whose keyword
 * filters the project list as it is typed, and the tags in use (`vocabulary`) to pick from; a
 * project must carry every one picked. Escape closes it and keeps the filter, and so does Enter in
 * the field (on a tag, Enter picks or drops it). `filter.tags` is already narrowed to the vocabulary,
 * so changes go up as updates to apply to the stored filter, which still holds the tags no project
 * carries. With a filter in force, the heading button clears it; while the popover is open, it stays
 * mounted as its fields change. */
export function ProjectFilterButton({
  filter,
  vocabulary,
  onChange,
}: {
  filter: ProjectFilter;
  vocabulary: string[];
  onChange: (update: FilterUpdate) => void;
}): React.ReactElement {
  const t = useT();
  const [open, setOpen] = useState(false);
  const filtering = isFiltering(filter);
  const clearAction = filtering && !open;
  const label = t(clearAction ? "sidebar.filter.clear" : "sidebar.filter.open");
  // The button never takes focus on a press, and react-aria hands focus back to it when the popover
  // closes, which would pull it off the terminal; the element the press came from gets it back.
  const pointerFocus = usePointerFocusReturn();
  const buttonHolder = useRef<HTMLDivElement>(null);
  const focusReturnListeners = useRef<AbortController | null>(null);
  useEffect(() => () => focusReturnListeners.current?.abort(), []);
  const keywordField = useTrimmedField(filter.keyword, (keyword) => onChange((f) => ({ ...f, keyword })));

  const onOpenChange = (isOpen: boolean) => {
    // A new gesture supersedes the previous close's delayed focus hand-off.
    focusReturnListeners.current?.abort();
    focusReturnListeners.current = null;
    if (isOpen && clearAction) {
      clear();
      return;
    }
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
    focusReturnListeners.current = listeners;
    holder.addEventListener("focusin", passOn, {
      once: true,
      signal: listeners.signal,
    });
    setTimeout(() => {
      listeners.abort();
      if (focusReturnListeners.current === listeners) focusReturnListeners.current = null;
    }, 500);
  };

  const clear = () => onChange(() => NO_FILTER);
  // Keep the trigger mounted when its action changes so clearing or closing preserves keyboard
  // focus. A clear action has no popup; the controlled open callback handles that press instead.
  const trigger = (
    <TitledControl title={label}>
      <Button
        isIconOnly
        size="sm"
        variant="ghost"
        aria-label={label}
        preventFocusOnPress
        aria-haspopup={clearAction ? false : undefined}
        className="row-icon-control rounded-md text-app-muted hover:text-foreground"
      >
        {filtering ? <FunnelX aria-hidden="true" className="size-4" /> : <Funnel aria-hidden="true" className="size-4" />}
      </Button>
    </TitledControl>
  );

  return (
    <div
      ref={buttonHolder}
      className="flex"
      onPointerDownCapture={pointerFocus.onPointerDownCapture}
    >
      <Popover isOpen={open} onOpenChange={onOpenChange}>
        {trigger}
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
                {...keywordField}
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
                  {/* HeroUI's close button hardcodes an English `aria-label` ahead of its spread
                      props, which also shadows the name react-aria supplies through the `clear`
                      slot, so the button is named from the catalog here. */}
                  <SearchField.ClearButton aria-label={t("common.clear")} />
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
  );
}

/** The tags in use, each a toggle: the tags picked are the ones a project must all carry. With
 * none in use, an empty panel says where tags are added. */
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
  const fade = useScrollFade({ className: "max-h-40 overflow-y-auto px-4 py-1" });
  // The default heading level: the popover it fills is named by a label rather than by a heading,
  // so there is none above it for the panel's own to sit level with.
  if (vocabulary.length === 0) return <EmptyPanel icon={Tags} message={t("sidebar.filter.noTags")} description={t("sidebar.filter.noTags.description")} />;
  return (
    <TagGroup
      aria-label={t("sidebar.filter.tags")}
      className="-mx-4"
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
      {/* The fade goes on the list, never on the popover around it: the popover is what animates
          away, and the fade's scroll-timeline animation never ends, so the popover's exit transition
          would wait for it forever. */}
      <TagGroup.List {...fade}>
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

/** How a chip takes its width: kept whole on the heading's line, where the line clips it, or
 * shrunk to the tooltip's line, where a long one breaks inside itself. */
const CHIP_FIT_CLASS = { whole: "shrink-0", wrapping: "min-w-0 max-w-full" };

/** One condition of the filter: the keyword in the neutral look, a picked tag in the accent one. */
function Chip({ kind, fit, children }: {
  kind: "keyword" | "tag";
  fit: keyof typeof CHIP_FIT_CLASS;
  children: string;
}): React.ReactElement {
  return <InlineChip size="xs" color={kind === "keyword" ? "default" : "accent"} dir="auto" wrap={fit === "wrapping"} className={CHIP_FIT_CLASS[fit]}>{children}</InlineChip>;
}

/** The whole filter as a tooltip: the same chips as the heading's line, wrapped onto as many lines
 * as the tooltip's width needs instead of clipped at one. Each chip is capped at the tooltip's
 * width, so a tag longer than a line breaks inside itself. The tooltip's surface
 * is a dialog's `--overlay`, so the chips take the fills of `.control-fills`: HeroUI's own
 * `--default` lands at 1.19:1 against it and the keyword would barely read as a chip. */
function ProjectFilterTooltip({ filter }: { filter: ProjectFilter }): React.ReactElement {
  return (
    <div className="control-fills flex flex-wrap gap-1.5">
      {filter.keyword !== "" && <Chip kind="keyword" fit="wrapping">{filter.keyword}</Chip>}
      {filter.tags.map((tag) => <Chip key={tag} kind="tag" fit="wrapping">{tag}</Chip>)}
    </div>
  );
}

/** The filter in force, on the Projects heading's line between its label and its controls: the
 * keyword first, in the neutral look, then the picked tags in the soft accent colour that sets them
 * apart from it. Nothing here is interactive; the heading's Funnel X clears the filter. The
 * conditions are one label to the pointer: clipped at the line's end, they fade there and run as
 * one marquee while the pointer is over them, carrying the whole filter as a tooltip of chips.
 * Each kind is a named group, so a screen reader hears what the words are. */
export function ProjectFilterSummary({ filter }: { filter: ProjectFilter }): React.ReactElement {
  const t = useT();
  return (
    <FadeOverflow className="flex min-w-0 flex-1 items-center gap-1.5" titleWhenClipped={<ProjectFilterTooltip filter={filter} />}>
      {filter.keyword !== "" && (
        <span role="group" aria-label={t("sidebar.filter.keyword")} className="inline-flex shrink-0">
          <Chip kind="keyword" fit="whole">{filter.keyword}</Chip>
        </span>
      )}
      {filter.tags.length > 0 && (
        <span role="group" aria-label={t("sidebar.filter.picked")} className="inline-flex shrink-0 gap-1.5">
          {filter.tags.map((tag) => (
            <Chip key={tag} kind="tag" fit="whole">{tag}</Chip>
          ))}
        </span>
      )}
    </FadeOverflow>
  );
}
