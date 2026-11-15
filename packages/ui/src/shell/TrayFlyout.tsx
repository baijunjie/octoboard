import React, { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

import { FadedScroller } from "../components/scroll/scrollFade";
import { isolate } from "../i18n/isolate";
import { textDirection } from "../i18n/languages";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { TrayFlyoutLine, TrayFlyoutModel } from "../platform";
import { usePlatform } from "../platform/react";
import { Badge, BadgeAnchor, MenuList, MenuSeparator, Tab, TabList, TabListContainer, Tabs, useTabPanelLink } from "../primitives";
import { FORCED_FOCUS_CLASS, lineRows, openerId } from "./TrayFlyoutLines";

const TAB_CLASS = `gap-1.5 forced-colors:ui-selected:underline forced-colors:ui-selected:decoration-2 ${FORCED_FOCUS_CLASS}`;

/**
 * The page of the Windows notification-area flyout (`?flyout=1`, see `windowKind`), which has no daemon: the shell
 * builds the model from the menu bar icon's (a tab per daemon; the chosen tab's sessions and quota rows; Check for
 * updates; the chosen tab's Open board, or Open in browser, and Disconnect for a remote daemon's tab; Quit completely),
 * and this page draws it and sends back what the user chooses. A quota line opens its rows in place, one at a time. Esc
 * closes the open one first, then the flyout. The shell shows the window and sizes it to what this page reports
 * (`setSize`); when it shows it, `onOpened` selects the tab to start on, collapses what was open and puts the focus on
 * that tab, or on the first line where there is no tab strip.
 */
export function TrayFlyout(): React.ReactElement {
  const { trayFlyout } = usePlatform();
  const t = useT();
  const language = useCurrentLanguage();
  const rtl = textDirection(language) === "rtl";
  const [model, setModel] = useState<TrayFlyoutModel | null>(null);
  // The model as last adopted, for `onOpened` to pick its tab from outside a state update.
  const heldModel = useRef<TrayFlyoutModel | null>(null);
  const [selected, setSelected] = useState<string>();
  const [expanded, setExpanded] = useState<string>();
  const [opening, setOpening] = useState(0);
  const [tabs, setTabs] = useState<ReadonlyMap<string, HTMLElement>>(new Map());
  const content = useRef<HTMLDivElement>(null);
  const panel = useRef<HTMLDivElement>(null);

  const adopt = useCallback((next: TrayFlyoutModel) => {
    heldModel.current = next;
    setModel(next);
    setSelected(next.tabs[next.chosen]?.key ?? next.tabs[0]?.key);
    setExpanded(undefined);
  }, []);
  useEffect(() => {
    if (!trayFlyout) return;
    let current = true;
    void trayFlyout.model().then((first) => {
      if (current && first) adopt(first);
    }, () => {});
    const unsubscribe = [
      trayFlyout.onModel(adopt),
      trayFlyout.onOpened(({ chosen }) => {
        const held = heldModel.current;
        if (held) setSelected(held.tabs[chosen]?.key ?? held.tabs[0]?.key);
        setExpanded(undefined);
        setOpening((count) => count + 1);
      }),
    ];
    return () => {
      current = false;
      for (const undo of unsubscribe) undo();
    };
  }, [trayFlyout, adopt]);

  // The content's size goes to the shell whenever it changes; it places the window against the taskbar by it.
  const sent = useRef("");
  useEffect(() => {
    const element = content.current;
    if (!element || !trayFlyout) return;
    const report = () => {
      const { width, height } = element.getBoundingClientRect();
      const size = { width: Math.ceil(width), height: Math.ceil(height) };
      const key = `${size.width}x${size.height}`;
      if (size.width === 0 || key === sent.current) return;
      sent.current = key;
      void trayFlyout.setSize(size).catch(() => {});
    };
    report();
    const observer = new ResizeObserver(report);
    observer.observe(element);
    return () => observer.disconnect();
  }, [trayFlyout, model]);

  const current = model?.tabs.find((tab) => tab.key === selected) ?? model?.tabs[0];
  const index = model && current ? model.tabs.indexOf(current) : -1;
  useTabPanelLink(panel, model && model.tabs.length > 1 && current ? tabs.get(current.key) : undefined);

  // After the flyout is shown: the selected tab takes the focus, or the first line where there is no strip.
  useEffect(() => {
    if (opening === 0 || !model || !current) return;
    const target = model.tabs.length > 1 ? tabs.get(current.key) : panel.current?.querySelector<HTMLElement>("[data-flyout-choosable]");
    target?.focus();
  }, [opening]);

  // The opener a collapse hands the focus back to, once its rows are gone from the menu.
  const refocus = useRef<string | undefined>(undefined);
  const opener = (id: string) => panel.current?.querySelector<HTMLElement>(`[data-flyout-opener="${CSS.escape(id)}"]`);
  useLayoutEffect(() => {
    if (expanded) opener(expanded)?.scrollIntoView?.({ block: "nearest" });
    else if (refocus.current) {
      opener(refocus.current)?.focus();
      refocus.current = undefined;
    }
  }, [expanded]);

  const collapse = () => {
    const open = expanded;
    if (!open) return;
    // Focus already on the opener stays there; one in the rows that go is handed to the opener.
    const focused = document.activeElement;
    refocus.current = focused && focused !== opener(open) ? open : undefined;
    setExpanded(undefined);
  };

  // Escape is taken in the capture phase: react-aria's menu answers it on the row and stops it, so a bubbling handler
  // would never see it.
  const onKeyDownCapture = (event: React.KeyboardEvent) => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    event.stopPropagation();
    if (expanded) collapse();
    else void trayFlyout?.close().catch(() => {});
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    const inMenu = event.target instanceof Element && event.target.closest("[role=menu]");
    if (!inMenu) return;
    const forward = rtl ? "ArrowLeft" : "ArrowRight";
    const back = rtl ? "ArrowRight" : "ArrowLeft";
    const row = (event.target as Element).closest<HTMLElement>("[data-flyout-opener], [data-flyout-owner]");
    if (event.key === forward && row?.dataset.flyoutOpener && expanded !== row.dataset.flyoutOpener) {
      event.preventDefault();
      setExpanded(row.dataset.flyoutOpener);
    } else if (event.key === back && expanded && (row?.dataset.flyoutOpener === expanded || row?.dataset.flyoutOwner === expanded)) {
      event.preventDefault();
      collapse();
    }
  };

  if (!model || !current) return <div ref={content} className="w-max" />;
  const context = { expanded, language };
  // A divider sets each run of lines off from the one before it; an empty run (a daemon held for the browser lists
  // nothing) leaves none behind.
  const rows: React.ReactElement[] = [];
  const runs: [string, TrayFlyoutLine[]][] = [
    ["p", model.panels[index] ?? []],
    ["s", model.shared],
    [`a-${current.key}-`, model.actions[index] ?? []],
    ["f", model.footer],
  ];
  runs.forEach(([scope, lines]) => {
    if (lines.length === 0) return;
    if (rows.length > 0 && (scope === "s" || scope === "f")) rows.push(<MenuSeparator key={`${scope}-separator`} />);
    rows.push(...lineRows(lines, scope, context));
  });

  const choose = (id: string) => {
    if (id.startsWith(openerId(""))) setExpanded((open) => (open === id ? undefined : id));
    else void trayFlyout?.choose(id).catch(() => {});
  };

  return (
    <FadedScroller className="h-dvh overflow-x-hidden bg-background text-foreground" onKeyDownCapture={onKeyDownCapture} onKeyDown={onKeyDown}>
      {/* 300 to 420px wide, which is what the shell is told: the page never scrolls sideways. */}
      <div ref={content} className="w-max min-w-75 max-w-105 border border-separator p-1">
        {model.tabs.length > 1 && (
          <Tabs variant="underline" selectedKey={current.key} onSelectionChange={(key) => { setSelected(key); setExpanded(undefined); }}>
            {/* Spanning the panel, so the strip's rule runs to its edge; the tabs keep their content width. */}
            <TabListContainer className="min-w-0 max-w-full" reserveScrollButtons>
              <TabList label={t("trayFlyout.tabs")} fit="content">
                {model.tabs.map((tab) => (
                  <Tab
                    key={tab.key}
                    id={tab.key}
                    size="content"
                    className={TAB_CLASS}
                    label={tab.raised ? t("trayFlyout.tabRaised", { name: isolate(tab.label) }) : undefined}
                    ref={(element) => {
                      if (element) setTabs((held) => (held.get(tab.key) === element ? held : new Map(held).set(tab.key, element)));
                    }}
                  >
                    <BadgeAnchor>
                      <span dir="auto" className="[overflow-wrap:anywhere]">{tab.label}</span>
                      {tab.raised && <Badge color="warning" size="xs" placement="top-right" />}
                    </BadgeAnchor>
                  </Tab>
                ))}
              </TabList>
            </TabListContainer>
          </Tabs>
        )}
        <div ref={panel} role={model.tabs.length > 1 ? "tabpanel" : undefined}>
          <MenuList label={current.label} onAction={choose}>{rows}</MenuList>
        </div>
      </div>
    </FadedScroller>
  );
}
