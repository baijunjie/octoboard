import { Tag as HeroTag, TagGroup as HeroTagGroup, tagVariants } from "@heroui/react";
import React from "react";

import { useDisabled } from "./busy";
import type { DataAttributes } from "./props";

/** HeroUI's tag group, in its small size, the one the application uses: a set of `Tag`s in a `TagList`, named by
 * `label`, one tab stop with the arrow keys moving between its tags. Its tags are toggles with
 * `selectionMode="multiple"`, the ones in `selectedKeys` picked, and each change of them reported to
 * `onSelectionChange` as whether a tag is now picked; or they are removable, with `onRemove` told of the tags
 * removed (by a tag's `TagRemoveButton`, or Delete or Backspace on a focused tag). The tags in `disabledKeys` can be
 * neither. */
export function TagGroup({ label, selectionMode, selectedKeys, onSelectionChange, disabledKeys, onRemove, className, children }: {
  label: string;
  selectionMode?: "multiple";
  selectedKeys?: string[];
  onSelectionChange?: (isSelected: (tag: string) => boolean) => void;
  disabledKeys?: string[];
  onRemove?: (tags: string[]) => void;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <HeroTagGroup
      aria-label={label}
      size="sm"
      className={className}
      selectionMode={selectionMode}
      selectedKeys={selectedKeys}
      onSelectionChange={onSelectionChange && ((keys) => onSelectionChange((tag) => keys === "all" || keys.has(tag)))}
      disabledKeys={disabledKeys}
      onRemove={onRemove && ((keys) => onRemove([...keys].map(String)))}
    >
      {children}
    </HeroTagGroup>
  );
}

/** The element holding a `TagGroup`'s tags. The other props, a `ref` among them, go to that element. */
export function TagList({ children, ...props }: Omit<React.HTMLAttributes<HTMLDivElement>, "children"> & DataAttributes & {
  ref?: React.Ref<HTMLDivElement>;
  children: React.ReactNode;
}): React.ReactElement {
  // React Aria's list takes the same DOM props, typed as its own.
  return <HeroTagGroup.List {...(props as object)}>{children}</HeroTagGroup.List>;
}

/** One tag of a `TagGroup`. `id` is its text and its key both. `children` is what it shows, or a function of whether
 * it is picked; a removable tag shows its own `TagRemoveButton` among them, as no other is added. */
export function Tag({ id, dir, className, children, ...data }: DataAttributes & {
  id: string;
  dir?: "ltr" | "rtl" | "auto";
  className?: string;
  children: React.ReactNode | ((state: { selected: boolean }) => React.ReactNode);
}): React.ReactElement {
  return (
    <HeroTag {...data} id={id} textValue={id} dir={dir} isDisabled={useDisabled()} className={className}>
      {/* Always the function form: HeroUI's tag looks for a remove button only among its direct children, and adds a
          default one beside a remove button placed further down. */}
      {({ isSelected }) => (typeof children === "function" ? children({ selected: isSelected }) : children)}
    </HeroTag>
  );
}

/** A removable tag's remove button, named by `label` (the tag's own text is added to the name by the tag). It is out of
 * the tab order, as the tag group is one tab stop: a tag is removed from the keyboard with Delete or Backspace on the
 * tag itself, which React Aria describes on it. `describedBy` `""` keeps a tooltip that repeats the name from
 * describing it. */
export function TagRemoveButton({ label, describedBy, className }: {
  label: string;
  describedBy?: string;
  className?: string;
}): React.ReactElement {
  return <HeroTag.RemoveButton excludeFromTabOrder isDisabled={useDisabled()} aria-label={label} aria-describedby={describedBy} className={className} />;
}

/** A small HeroUI tag drawn on its own, outside a `TagGroup` (a read-only summary), which would otherwise bring a
 * collection and row focus along: a `span` with the tag's classes and its part's mark, so it is drawn as a group's
 * tag is. */
export function StaticTag({ dir, className, children, ...data }: DataAttributes & {
  dir?: "auto";
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return <span {...data} data-slot="tag" dir={dir} className={tagVariants({ size: "sm" }).base({ className })}>{children}</span>;
}
