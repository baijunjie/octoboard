import { Tabs as HeroTabs } from "@heroui/react";
import React, { useLayoutEffect } from "react";

import { useDisabled } from "./busy";
import type { DataAttributes } from "./props";
import { useTabScrollLabels } from "./tabScrollLabels";

/** A tab strip and its panels: HeroUI's tabs. `filled` (the default) is HeroUI's filled strip; `underline` marks the
 * selected tab with a line under it. A key is a tab's and its panel's `id`; with `selectedKey` the selection is the
 * caller's, told of changes by `onSelectionChange`, and with `defaultSelectedKey` it is the strip's own. */
export function Tabs({ variant = "filled", orientation, selectedKey, defaultSelectedKey, onSelectionChange, className, children }: {
  variant?: "filled" | "underline";
  orientation?: "horizontal" | "vertical";
  selectedKey?: string;
  defaultSelectedKey?: string;
  onSelectionChange?: (key: string) => void;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <HeroTabs
      variant={variant === "underline" ? "secondary" : "primary"}
      orientation={orientation}
      isDisabled={useDisabled()}
      selectedKey={selectedKey}
      defaultSelectedKey={defaultSelectedKey}
      onSelectionChange={onSelectionChange && ((key) => onSelectionChange(String(key)))}
      className={className}
    >
      {children}
    </HeroTabs>
  );
}

/** The width treatment of a horizontal strip of text tabs (a label with at most an icon or a mark beside it), from the
 * strip's start at their content's width: shared so such a strip looks the same wherever it is built. */
const CONTENT_CONTAINER_CLASS = "w-max max-w-full self-start";
/** The padding of HeroUI's filled list, restored on its underline list, which drops it: exactly the focus ring and its
 * offset (2px each), the room they need inside the scroller's clip. */
const CONTENT_LIST_CLASS = "w-max min-w-0 p-1";
/** A tab of such a strip: a scroll margin (so a revealed tab keeps its ring in view), one line, and its label taking
 * the foreground colour on hover. */
const STRIP_TAB_CLASS = "scroll-mx-1 whitespace-nowrap hover:text-foreground";
/** HeroUI marks the edges its scroll buttons sit at on the container (`data-left-scroll`, `data-right-scroll`, and
 * `data-left-right-scroll` for both). */
const RESERVE_SCROLL_BUTTONS_CLASS = "has-[>:is([data-left-scroll=true],[data-left-right-scroll=true])]:ps-6 has-[>:is([data-right-scroll=true],[data-left-right-scroll=true])]:pe-6";
const TAB_SIZE_CLASS = {
  content: `w-auto flex-none ${STRIP_TAB_CLASS}`,
  // Short enough to sit in a pane header's row beside the pane's title with its focus ring, and narrow enough to leave
  // the title its room.
  compact: `h-6 px-1.5 ${STRIP_TAB_CLASS}`,
};

/** The scrolling container of a strip's list, with its scroll buttons named in the current language (see
 * `useTabScrollLabels`). `fit="content"` makes it as wide as its tabs, from its row's start, scrolling when they do not
 * fit. `after` is placed behind the container's content, inside its element, for what is laid over the strip.
 * HeroUI lays its scroll buttons over the strip's ends; `reserveScrollButtons` keeps the strip's content clear of them by
 * padding only the edge that has a scroll button. */
export function TabListContainer({ fit, after, reserveScrollButtons, className, ...props }: Omit<React.HTMLAttributes<HTMLDivElement>, "children"> & DataAttributes & {
  fit?: "content";
  after?: React.ReactNode;
  reserveScrollButtons?: boolean;
  children?: React.ReactNode;
}): React.ReactElement {
  const tabScrollLabels = useTabScrollLabels();
  const classes = [fit === "content" && CONTENT_CONTAINER_CLASS, className, reserveScrollButtons && RESERVE_SCROLL_BUTTONS_CLASS].filter(Boolean).join(" ");
  return (
    <HeroTabs.ListContainer
      {...props}
      className={classes || undefined}
      render={(rendered) => <div {...rendered}>{tabScrollLabels(rendered.children)}{after}</div>}
    />
  );
}

/** A strip's list of tabs, named `label` for assistive technology. `fit="content"` sizes it to its tabs and pads it for
 * their focus rings, for a strip of `Tab size="content"` or `"compact"`. */
export function TabList({ label, fit, ref, className, children }: {
  label: string;
  fit?: "content";
  ref?: React.Ref<HTMLDivElement>;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <HeroTabs.List ref={ref} aria-label={label} className={fit === "content" ? `${CONTENT_LIST_CLASS} ${className ?? ""}` : className}>
      {children}
    </HeroTabs.List>
  );
}

/** Kept on every tab, whichever strip it is in. A hovered tab keeps full opacity where HeroUI dims it, matching both of
 * HeroUI's hover selectors: react-aria's `data-hovered` outlives `:hover` by a frame when the pointer leaves, and dimming
 * in that frame starts an opacity transition that has WKWebView repaint the whole masked strip, shaking every tab's icon
 * and label (an approved exception, `docs/ui-standard.md`'s "Confirmed visual choices"). And every tab keeps its own
 * composited layer, so WKWebView does not shift its rasterized icon and label between pixel rows while the selection
 * indicator moves inside the strip's masked scroller; HeroUI's tab is already `position: relative; z-index: 1`, so the
 * transform's only other effect is that the tab contains its `position: fixed` descendants. */
// The new indicator stays in its selected tab's stacking context. SharedElement moves it over other tabs on the way
// forward, so keep that whole context below the unselected tabs and their labels.
const TAB_CLASS = "[&:not([data-selected=true]):not([data-disabled=true]):is(:hover,[data-hovered=true])]:opacity-100 transform-[translateZ(0)] ui-selected:z-0";

/** One tab, keyed `id`, with the selection indicator inside it unless `indicator` is false; `indicatorClassName` moves
 * the indicator (an underline strip's to its list's edge). `size` is a text strip's tab (see `TabList`'s `fit`):
 * `content` at its content's width, `compact` for a strip in a pane header. `label` replaces the accessible name its
 * content gives it. */
export function Tab({ id, label, size, indicator = true, indicatorClassName, ref, className, onHoverChange, onFocus, onBlur, children, ...data }: DataAttributes & {
  id: string;
  label?: string;
  size?: "content" | "compact";
  indicator?: boolean;
  indicatorClassName?: string;
  ref?: React.Ref<HTMLDivElement>;
  className?: string;
  onHoverChange?: (hovered: boolean) => void;
  onFocus?: () => void;
  onBlur?: () => void;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <HeroTabs.Tab
      {...data}
      id={id}
      ref={ref}
      aria-label={label}
      onHoverChange={onHoverChange}
      onFocus={onFocus && (() => onFocus())}
      onBlur={onBlur && (() => onBlur())}
      className={[TAB_CLASS, size && TAB_SIZE_CLASS[size], className].filter(Boolean).join(" ")}
    >
      {children}
      {indicator && <HeroTabs.Indicator className={indicatorClassName} />}
    </HeroTabs.Tab>
  );
}

/** The panel of the tab keyed `id`. It is mounted only while its tab is selected, unless `keepMounted`: then it stays
 * mounted while unselected, hidden, inert and out of the tab order. `hidden` hides it, still mounted, while selected. */
export function TabPanel({ id, keepMounted, hidden, className, children }: {
  id: string;
  keepMounted?: boolean;
  hidden?: boolean;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <HeroTabs.Panel id={id} shouldForceMount={keepMounted} hidden={hidden} className={className}>
      {children}
    </HeroTabs.Panel>
  );
}

/**
 * Makes `panel`, an element of the caller's own, the panel of `tab` (a tab element of this strip, `undefined` while none
 * is shown): the tab names it in `aria-controls`, as it does its `TabPanel`, so the panel takes the id react-aria gave
 * the tab for it, and is labelled by the tab. For a panel that holds the content of every tab at once, which a
 * `TabPanel` cannot, as it renders only the content of its own. With no tab the panel carries neither. It runs after
 * every render, since a tab that arrives in a commit of its own (react-aria renders the tabs after the strip) is passed
 * in by the caller's next one.
 */
export function useTabPanelLink(panel: React.RefObject<HTMLElement | null>, tab: HTMLElement | undefined): void {
  useLayoutEffect(() => {
    const element = panel.current;
    if (!element) return;
    const panelId = tab?.getAttribute("aria-controls");
    if (tab && panelId) {
      element.setAttribute("id", panelId);
      element.setAttribute("aria-labelledby", tab.id);
    } else {
      element.removeAttribute("id");
      element.removeAttribute("aria-labelledby");
    }
  });
}
