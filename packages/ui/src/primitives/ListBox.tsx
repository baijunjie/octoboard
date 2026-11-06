import { ListBox as HeroListBox } from "@heroui/react";
import React, { useCallback } from "react";
import { ListLayout, Text, Virtualizer } from "react-aria-components";

import { useDisabled } from "./busy";
import { type DataAttributes, definedProps } from "./props";

/** HeroUI's list box: a list of `ListBoxItem`s, named `label` unless what holds it names it (a select's popover).
 * `onAction` receives the id of an item activated (a click, Enter). With `items` the list is built from them by the
 * `children` function and rebuilt when `items` or `dependencies` change, as react-aria caches each item's row
 * otherwise. With `rowHeight` the list is virtualized, every row that tall, for a list too long to mount whole. The rest
 * of the props (a scroll fade's, a list motion's) go on the list's element. */
export function ListBox<T extends object>({ ref, label, items, dependencies, onAction, rowHeight, className, children, ...rest }: DataAttributes & Omit<React.HTMLAttributes<HTMLDivElement>, "children"> & {
  ref?: React.Ref<HTMLDivElement>;
  label?: string;
  items?: Iterable<T>;
  dependencies?: readonly unknown[];
  onAction?: (id: string) => void;
  rowHeight?: number;
  className?: string;
  children: React.ReactNode | ((item: T) => React.ReactElement);
}): React.ReactElement {
  const list = (
    <HeroListBox<T>
      {...(rest as object)}
      ref={ref}
      {...definedProps({
        "aria-label": label,
        items,
        dependencies,
        onAction: onAction && ((key: string | number) => onAction(String(key))),
        className,
      })}
    >
      {children}
    </HeroListBox>
  );
  if (rowHeight === undefined) return list;
  return (
    <Virtualizer layout={ListLayout} layoutOptions={{ rowHeight }}>
      {list}
    </Virtualizer>
  );
}

/** One item of a `ListBox`, keyed `id`: `textValue` is what typing to select matches, `label` replaces the accessible
 * name its content gives it. With `indicator` it ends in HeroUI's check mark, shown while it is the chosen one (in a
 * select). A `disabled` item is announced as unavailable (`aria-disabled`) however its list treats it: a select keeps
 * it focusable (see `Select`), where react-aria sets no such mark and drops one given to it, so it is set on the
 * element here. */
export function ListBoxItem({ id, textValue, label, disabled: ownDisabled, indicator, className, style, onFocusCapture, onBlurCapture, children, ...data }: DataAttributes & {
  id: string;
  textValue: string;
  label?: string;
  disabled?: boolean;
  indicator?: boolean;
  className?: string;
  style?: React.CSSProperties;
  onFocusCapture?: (event: React.FocusEvent<HTMLElement>) => void;
  onBlurCapture?: (event: React.FocusEvent<HTMLElement>) => void;
  children: React.ReactNode;
}): React.ReactElement {
  const disabled = useDisabled(ownDisabled);
  const markDisabled = useCallback((node: HTMLElement | null) => {
    if (!node) return;
    if (disabled) node.setAttribute("aria-disabled", "true");
    else node.removeAttribute("aria-disabled");
  }, [disabled]);
  return (
    <HeroListBox.Item
      {...data}
      ref={markDisabled}
      id={id}
      textValue={textValue}
      {...definedProps({ "aria-label": label, isDisabled: disabled, className, style })}
      {...(definedProps({ onFocusCapture, onBlurCapture }) as object)}
    >
      {children}
      {indicator && <HeroListBox.ItemIndicator />}
    </HeroListBox.Item>
  );
}

/** The part of a `ListBoxItem` that is its name, for an item that holds more than its name (a `Description`, a note):
 * the rest is then left out of the name. */
export function ListBoxItemLabel({ className, children }: { className?: string; children: React.ReactNode }): React.ReactElement {
  return <Text slot="label" className={className}>{children}</Text>;
}
