import React from "react";
import {
  Button as AriaButton,
  Collection as AriaCollection,
  ListLayout,
  Rect,
  Tree as AriaTree,
  TreeHeader as AriaTreeHeader,
  TreeItem as AriaTreeItem,
  TreeItemContent as AriaTreeItemContent,
  TreeSection as AriaTreeSection,
  Virtualizer,
} from "react-aria-components";

import { useDisabled } from "./busy";
import { type DataAttributes, definedProps } from "./props";

// HeroUI 3 has no tree, so these wrap the react-aria-components `Tree` HeroUI itself builds on, always virtualized:
// arrows, Home / End, type-ahead, Left / Right to collapse and expand a row that has rows under it.

/**
 * A virtualized tree of `TreeItem`s, every row `rowHeight` tall, named `label`. `items` are built into rows by the
 * `children` function, and rebuilt when `items` or `dependencies` change, as react-aria caches each row otherwise.
 * `expandedKeys` are the ids of the rows shown open; `onExpandedChange` receives the ids to show open after a row was
 * opened or closed from the keyboard or its chevron, `onAction` the id of a row activated (a click, Enter).
 *
 * With `headingHeight` the tree is in sections (`TreeSection`), each headed by a `TreeHeader` that tall which sticks
 * to the top of the list while its section is in view. The rest of the props (a scroll fade's, a list motion's) go on
 * the tree's scrolling element.
 */
export function Tree<T extends object>({ ref, label, items, expandedKeys, onExpandedChange, onAction, dependencies, rowHeight, headingHeight, className, children, onKeyDown, ...rest }: DataAttributes & Omit<React.HTMLAttributes<HTMLDivElement>, "children"> & {
  ref?: React.Ref<HTMLDivElement>;
  label: string;
  items: Iterable<T>;
  expandedKeys: ReadonlySet<string>;
  onExpandedChange: (keys: Set<string>) => void;
  onAction: (key: string) => void;
  dependencies?: readonly unknown[];
  rowHeight: number;
  headingHeight?: number;
  className?: string;
  children: (item: T) => React.ReactElement;
}): React.ReactElement {
  const sectioned = headingHeight !== undefined;
  return (
    <Virtualizer layout={sectioned ? StickySectionLayout : PersistedTreeLayout} layoutOptions={sectioned ? { rowHeight, headingHeight } : { rowHeight }}>
      <AriaTree<T>
        {...(rest as object)}
        ref={ref}
        aria-label={label}
        items={items}
        expandedKeys={expandedKeys as Set<string>}
        onExpandedChange={(keys) => onExpandedChange(new Set([...keys].map(String)))}
        onAction={(key) => onAction(String(key))}
        render={(props) => <div {...props} onKeyDown={(event) => {
          props.onKeyDown?.(event);
          // React Aria moves focus but leaves these keys' default scrolling enabled without selection. WebKit's
          // native keyboard scroll then conflicts with the virtualizer's scroll, leaving its painted offset behind.
          if (["Home", "End", "PageUp", "PageDown"].includes(event.key) && !event.metaKey && !event.ctrlKey && !event.altKey &&
            !(event.target instanceof Element && event.target.closest('input, textarea, select, [contenteditable]:not([contenteditable="false"])'))) {
            event.preventDefault();
          }
          onKeyDown?.(event);
        }} />}
        {...definedProps({ dependencies, className: sectioned ? `${className ?? ""} ${STICKY_SECTION_SCROLL_CLASS}` : className })}
      >
        {children}
      </AriaTree>
    </Virtualizer>
  );
}

/** The rows of `items`, built by the `children` function: a `TreeItem`'s child rows, or a `TreeSection`'s rows,
 * rebuilt when `items` or `dependencies` change. */
export function TreeCollection<T extends object>({ items, dependencies, children }: {
  items: Iterable<T>;
  dependencies?: readonly unknown[];
  children: (item: T) => React.ReactElement;
}): React.ReactElement {
  return (
    <AriaCollection items={items} {...definedProps({ dependencies })}>
      {children}
    </AriaCollection>
  );
}

/** One row of a `Tree`, keyed `id`: `textValue` is what type-ahead matches, `label` replaces the accessible name its
 * content gives it. `content` is what the row shows, or a function of whether the row is open; `children` are its
 * child rows (a `TreeCollection`), and a row that can hold rows says so with `expandable`. `expandable` reaches only the
 * row's chevron: react-aria decides whether a row is expandable (its `aria-expanded`, Left and Right) from the children
 * it holds, so a row that holds none yet is not expandable however it is marked. A caller whose rows are built when
 * opened gives such a row a stand-in child row until they are. */
export function TreeItem({ ref, id, textValue, label, expandable, className, content, children, ...data }: DataAttributes & {
  ref?: React.Ref<HTMLDivElement>;
  id: string;
  textValue: string;
  label?: string;
  expandable?: boolean;
  className?: string;
  content: React.ReactNode | ((state: { expanded: boolean }) => React.ReactNode);
  children?: React.ReactNode;
}): React.ReactElement {
  return (
    <AriaTreeItem {...data} ref={ref} id={id} textValue={textValue} {...definedProps({ "aria-label": label, hasChildItems: expandable, isDisabled: useDisabled(), className })}>
      <AriaTreeItemContent>
        {typeof content === "function" ? ({ isExpanded }) => content({ expanded: isExpanded }) : content}
      </AriaTreeItemContent>
      {children}
    </AriaTreeItem>
  );
}

/** The control in a `TreeItem`'s content that opens or closes it, named `label` (react-aria appends the row's name).
 * It has none of HeroUI's button look: a hover fill or a press shrink would mark it apart from the row it belongs to. */
export function TreeExpandButton({ label, className, children }: { label: string; className?: string; children: React.ReactNode }): React.ReactElement {
  return (
    <AriaButton slot="chevron" aria-label={label} isDisabled={useDisabled()} {...definedProps({ className })}>
      {children}
    </AriaButton>
  );
}

/** A section of a sectioned `Tree` (one given `headingHeight`), keyed `id`: its `TreeHeader` and its rows. */
export function TreeSection({ id, className, children }: { id: string; className?: string; children: React.ReactNode }): React.ReactElement {
  return (
    <AriaTreeSection id={id} {...definedProps({ className })}>
      {children}
    </AriaTreeSection>
  );
}

/** A `TreeSection`'s heading, which sticks to the top of the list while its section is in view. */
export function TreeHeader({ className, children, ...data }: DataAttributes & { className?: string; children: React.ReactNode }): React.ReactElement {
  return (
    <AriaTreeHeader {...data} {...definedProps({ className })}>
      {children}
    </AriaTreeHeader>
  );
}

/** A listing that grows can leave a cached offscreen row outside ListLayout's current root traversal. The upstream
 * persisted-key check sees the cache entry and skips rebuilding, so a focused row never mounts. Rebuild only when an
 * existing persisted item is missing from that traversal; its cached position may also predate the new rows. */
class PersistedTreeLayout extends ListLayout<unknown> {
  protected override layoutIfNeeded(rect: Rect): void {
    super.layoutIfNeeded(rect);
    const virtualizer = this.virtualizer;
    if (!virtualizer || virtualizer.persistedKeys.size === 0) return;
    const roots = new Set(this.rootNodes.map((node) => node.layoutInfo.key));
    const nodes = [...this.rootNodes];
    for (const node of nodes) {
      roots.add(node.layoutInfo.key);
      if (node.children) nodes.push(...node.children);
    }
    const missing = [...virtualizer.persistedKeys].some((key) => virtualizer.collection.getItem(key)?.type === "item" && !roots.has(key));
    if (!missing) return;
    // Match ListLayout's missing-cache fallback, recalculating the row instead of returning its stale layout info.
    this.requestedRect = new Rect(0, 0, Infinity, Infinity);
    this.rootNodes = this.buildCollection();
    this.requestedRect = new Rect(0, 0, this.contentSize.width, this.contentSize.height);
  }
}

/** The layout of a sectioned tree. The virtualizer's sticky wrapper must stay within its own section, whose overflow
 * is visible. Render copies leave the layout's original positions intact for keyboard navigation and row offsets. */
export class StickySectionLayout extends ListLayout<unknown> {
  /**
   * Lays out every row and heading at once, which their fixed heights make exact and cheap, where
   * `ListLayout` (react-stately 3.50) lays out only what lies around the visible rectangle and
   * estimates a section's rest. Inside a `Tree` that estimate counts the content node of every
   * expanded row still to come as one more row, and the virtualizer sizes the scroller from it,
   * keeping that size while scrolling lays the rest out; scrolled into the excess, the section has
   * already ended, and its heading, which sticks only within it, is gone with its rows. And a section
   * laid out again after the scroller resizes skips every child above the visible rectangle, its own
   * heading among them, so a section scrolled past its top loses its heading. Starting from the top
   * of the list every time leaves nothing to estimate or skip.
   */
  protected override buildCollection(offset?: number): ReturnType<ListLayout<unknown>["buildCollection"]> {
    this.requestedRect = new Rect(0, 0, Infinity, Infinity);
    const nodes = super.buildCollection(offset);
    this.requestedRect = new Rect(0, 0, this.contentSize.width, this.contentSize.height);
    return nodes;
  }

  override getVisibleLayoutInfos(rect: Parameters<ListLayout<unknown>["getVisibleLayoutInfos"]>[0]): ReturnType<ListLayout<unknown>["getVisibleLayoutInfos"]> {
    return super.getVisibleLayoutInfos(rect).map((info) => {
      if (info.type !== "section" && info.type !== "header") return info;
      const rendered = info.copy();
      if (info.type === "section") rendered.allowOverflow = true;
      else {
        rendered.isSticky = true;
        rendered.rect.y = 0;
        rendered.zIndex = 3;
      }
      return rendered;
    });
  }
}

/** Virtualizer section wrappers otherwise establish a stacking context through inline layout containment, trapping
 * their sticky headings below the scroller's overlay fades. Size and style containment remain; the explicit section
 * rectangle still bounds the heading's sticky travel. */
const STICKY_SECTION_SCROLL_CLASS = "[&_[role=presentation]:has(>[role=rowgroup])]:[contain:size_style]! [&_[role=presentation]:has(>[role=rowgroup])]:z-auto!";
