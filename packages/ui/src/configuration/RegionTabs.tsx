import { Tabs } from "@heroui/react";
import { TriangleAlert } from "lucide-react";
import React, { useRef, useState } from "react";
import { useFocusVisible } from "react-aria";

import { AnchoredTooltip } from "../components/AnchoredTooltip";
import { useTabScrollLabels } from "../components/tabScrollLabels";
import { useListMotion } from "../components/useListMotion";
import { useT } from "../i18n/react";

export type Region = "consoles" | "accounts" | "settings";

/** One region of a configuration document, as the import's review and result show it. */
export interface RegionPanel {
  id: Region;
  /** The tab's name. */
  label: string;
  /** The panel holds something that needs attention (a warning or a failure): a glyph follows the label. */
  attention: boolean;
  content: React.ReactNode;
}

const PANEL_CLASS = "mt-0 flex flex-col gap-4 p-0";

/**
 * The regions of a configuration document as tabs, a panel for each: the consoles and projects, the accounts and the
 * application settings, each present only when the document carries it. With a single region there is no tab bar and
 * the content stands as it is; with none there is nothing. A tab whose panel needs attention carries a warning glyph
 * after its label and says so in its accessible name and in a tooltip beside the tab. Tabs that come and go move with
 * the shared list motion. The selection is the caller's (`selected`), which can then move it to the tab holding a
 * problem; a selection that is not among the regions is the first of them. A panel is mounted only while its tab is
 * selected, so what must outlive a switch is the caller's state, not the panel's.
 */
export function RegionTabs({
  label,
  regions,
  selected,
  onSelect,
}: {
  /** The tab list's accessible name. */
  label: string;
  regions: RegionPanel[];
  selected: Region | undefined;
  onSelect: (region: Region) => void;
}): React.ReactElement | null {
  const tabScrollLabels = useTabScrollLabels();
  const listRef = useListMotion<HTMLDivElement>({ contentWidth: true });
  if (regions.length === 0) return null;
  if (regions.length === 1) return <>{regions[0].content}</>;
  const current = regions.some((region) => region.id === selected) ? selected : regions[0].id;
  return (
    <Tabs selectedKey={current} onSelectionChange={(key) => onSelect(key as Region)} className="gap-4">
      <Tabs.ListContainer className="w-max max-w-full self-start" render={(props) => <div {...props}>{tabScrollLabels(props.children)}</div>}>
        {/* Positioned, so a departing tab's visual stays in the strip's scroll. */}
        <Tabs.List ref={listRef} aria-label={label} className="relative w-max min-w-0 p-1.5">
          {regions.map((region) => (
            <RegionTab key={region.id} region={region} />
          ))}
        </Tabs.List>
      </Tabs.ListContainer>
      {regions.map((region) => (
        <Tabs.Panel key={region.id} id={region.id} className={PANEL_CLASS}>
          {region.content}
        </Tabs.Panel>
      ))}
    </Tabs>
  );
}

/**
 * One region's tab. One that needs attention is named so (see `RegionTabs`) and a tooltip beside the tab says it, an
 * `AnchoredTooltip` placed against the tab itself, since the tab is the control and nothing can be nested in it, so
 * the glyph is decorative. It opens while the pointer is on the tab and while the tab has keyboard focus (by
 * react-aria's input modality, not `:focus-visible`, so a press with the mouse that focuses the tab does not). The
 * tab's accessible name already says it needs attention, so the tooltip is not linked to the tab.
 */
function RegionTab({ region }: { region: RegionPanel }): React.ReactElement {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null);
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const { isFocusVisible } = useFocusVisible();
  return (
    <Tabs.Tab
      ref={ref}
      id={region.id}
      data-list-motion={region.id}
      aria-label={region.attention ? t("configuration.import.tab.attention", { tab: region.label }) : undefined}
      onHoverChange={setHovered}
      onFocus={() => setFocused(true)}
      onBlur={() => setFocused(false)}
      // Hovered labels take the foreground colour for clear feedback.
      className="w-auto flex-none scroll-mx-1.5 gap-2 whitespace-nowrap hover:text-foreground"
    >
      {region.label}
      {region.attention && (
        <>
          <TriangleAlert aria-hidden="true" className="size-3.5 shrink-0 text-warning-soft-foreground" />
          <AnchoredTooltip tooltip={t("configuration.import.tab.attentionTip")} anchor={ref} placement="end" hovered={hovered || (focused && isFocusVisible)} immediate={focused} />
        </>
      )}
      <Tabs.Indicator />
    </Tabs.Tab>
  );
}
