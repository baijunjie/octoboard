import { Tag as HeroTag, TagGroup as HeroTagGroup, tagVariants } from "@heroui/react";
import React, { useId, useLayoutEffect, useRef } from "react";

import { useDisabled } from "./busy";
import { type DataAttributes, definedProps } from "./props";

/** HeroUI's tag group, in its small size, the one the application uses: a set of `Tag`s in a `TagList`, named by
 * `label`, one tab stop with the arrow keys moving between its tags. Its tags are toggles with
 * `selectionMode="multiple"`, the ones in `selectedKeys` picked, and each change of them reported to
 * `onSelectionChange` as whether a tag is now picked; or they are removable, with `onRemove` told of the tags
 * removed (by a tag's `TagRemoveButton`, or Delete or Backspace on a focused tag). Each tag may also carry an action
 * of its own (`Tag`'s `onAction`). The tags in `disabledKeys` can be neither. */
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

/** One tag of a `TagGroup`. `id` is its key, and its text unless `textValue` gives another. `children` is what it
 * shows, or a function of whether it is picked; a removable tag shows its own `TagRemoveButton` among them, as no other
 * is added. `onAction` is the tag's own action, run by a press on it or Enter while it has focus (opening what it stands
 * for), apart from a removable tag's remove button and its Delete or Backspace; `description` says what that action
 * does to assistive technology, read after the tag's own description (how to remove it). */
export function Tag({ id, textValue, dir, className, onAction, description, children, ...data }: DataAttributes & {
  id: string;
  textValue?: string;
  dir?: "ltr" | "rtl" | "auto";
  className?: string;
  onAction?: () => void;
  description?: string;
  children: React.ReactNode | ((state: { selected: boolean }) => React.ReactNode);
}): React.ReactElement {
  return (
    <HeroTag {...data} {...definedProps({ onAction })} id={id} textValue={textValue ?? id} dir={dir} isDisabled={useDisabled()} className={className}>
      {/* Always the function form: HeroUI's tag looks for a remove button only among its direct children, and adds a
          default one beside a remove button placed further down. */}
      {({ isSelected }) => {
        const content = typeof children === "function" ? children({ selected: isSelected }) : children;
        // Beside the caller's elements rather than around them, which would wrap the tag's parts in an element of ours.
        return description === undefined ? content : [...React.Children.toArray(content), <TagDescription key="description" text={description} />];
      }}
    </HeroTag>
  );
}

/** A tag's description, the hidden text its row is described by. React Aria renders a collection's items twice, the
 * second time from the collection it built without calling `Tag` again, and writes the row's `aria-describedby` itself
 * (how to remove the tag, while the keyboard is in use): so the id is added from inside the row's own content, which is
 * rendered again with the row, after React Aria's value on every commit, and taken back as the description goes. The
 * render function holding it runs only inside the row (the pass that builds the collection does not call it), so a row
 * is always there; the `if (row)` guards are a safety net. */
function TagDescription({ text }: { text: string }): React.ReactElement {
  const id = useId();
  const ref = useRef<HTMLSpanElement>(null);
  const describe = (row: Element, on: boolean) => {
    const ids = (row.getAttribute("aria-describedby") ?? "").split(" ").filter((value) => value !== "" && value !== id);
    if (on) ids.push(id);
    if (ids.length > 0) row.setAttribute("aria-describedby", ids.join(" "));
    else row.removeAttribute("aria-describedby");
  };
  useLayoutEffect(() => {
    const row = ref.current?.closest('[role="row"]');
    if (row) describe(row, true);
  });
  useLayoutEffect(() => {
    const row = ref.current?.closest('[role="row"]');
    return () => {
      if (row) describe(row, false);
    };
  }, []);
  return <span ref={ref} id={id} hidden>{text}</span>;
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
