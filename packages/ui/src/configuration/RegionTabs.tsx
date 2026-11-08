import { TriangleAlert } from "lucide-react";
import React, { useRef, useState } from "react";

import { AnchoredTooltip } from "../components/hint/AnchoredTooltip";
import { Tab, TabList, TabListContainer, TabPanel, Tabs, useKeyboardModality } from "../primitives";
import { useListMotion } from "../components/motion/useListMotion";
import { useT } from "../i18n/react";

export type Region = "consoles" | "settings" | "accounts";

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
 * The regions of a configuration document as tabs, a panel for each: the consoles and projects, the application settings
 * and the accounts, each present only when the document carries it. With a single region there is no tab bar and
 * the content stands as it is, unless `alwaysTabs` keeps the bar for it (the import's review does, whose tabs follow
 * the checked parts); with none there is nothing. A tab whose panel needs attention carries a warning glyph
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
  alwaysTabs = false,
}: {
  /** The tab list's accessible name. */
  label: string;
  regions: RegionPanel[];
  selected: Region | undefined;
  onSelect: (region: Region) => void;
  /** Keeps the tab bar for a single region, in place of letting its content stand alone. */
  alwaysTabs?: boolean;
}): React.ReactElement | null {
  const listRef = useListMotion<HTMLDivElement>({ contentWidth: true });
  if (regions.length === 0) return null;
  if (regions.length === 1 && !alwaysTabs) return <>{regions[0].content}</>;
  const current = regions.some((region) => region.id === selected) ? selected : regions[0].id;
  return (
    <Tabs selectedKey={current} onSelectionChange={(key) => onSelect(key as Region)} className="gap-4">
      <TabListContainer fit="content">
        {/* Positioned, so a departing tab's visual stays in the strip's scroll. */}
        <TabList ref={listRef} label={label} fit="content" className="relative">
          {regions.map((region) => (
            <RegionTab key={region.id} region={region} />
          ))}
        </TabList>
      </TabListContainer>
      {regions.map((region) => (
        <TabPanel key={region.id} id={region.id} className={PANEL_CLASS}>
          {region.content}
        </TabPanel>
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
  const isFocusVisible = useKeyboardModality();
  return (
    <Tab
      ref={ref}
      id={region.id}
      data-list-motion={region.id}
      label={region.attention ? t("configuration.import.tab.attention", { tab: region.label }) : undefined}
      onHoverChange={setHovered}
      onFocus={() => setFocused(true)}
      onBlur={() => setFocused(false)}
      size="content"
      className="gap-2"
    >
      {region.label}
      {region.attention && (
        <>
          <TriangleAlert aria-hidden="true" className="size-3.5 shrink-0 text-warning-soft-foreground" />
          <AnchoredTooltip tooltip={t("configuration.import.tab.attentionTip")} anchor={ref} placement="end" hovered={hovered || (focused && isFocusVisible)} immediate={focused} />
        </>
      )}
    </Tab>
  );
}
