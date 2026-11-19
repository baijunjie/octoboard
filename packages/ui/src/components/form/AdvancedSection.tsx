import React from "react";

import { StatusAnnouncer } from "../announce/StatusAnnouncer";
import { Disclosure, DisclosureHeading, TabPanel, Tabs } from "../../primitives";
import { FlushDisclosureContent, QuietDisclosureTrigger } from "../disclosure/QuietDisclosure";
import { TextTabStrip } from "../tabs/TextTabStrip";
import { useT } from "../../i18n/react";

/** The collapsed Advanced section of a dialog, folded away by default as what is inside is rarely
 * set, under a `QuietDisclosureTrigger`, so at rest it reads as an action rather than a field label
 * set in from the others. */
export function AdvancedSection({
  expanded,
  onExpandedChange,
  status,
  children,
}: {
  expanded: boolean;
  onExpandedChange: (expanded: boolean) => void;
  /** The model field's note, said from a status region: it changes with no action of the user's,
   * and its picker is locked while it says "loading", so it cannot be reached to be read. */
  status?: string;
  children: React.ReactNode;
}): React.ReactElement {
  const t = useT();
  return (
    <Disclosure expanded={expanded} onExpandedChange={onExpandedChange}>
      <DisclosureHeading>
        <QuietDisclosureTrigger>{t("dialog.agentSettings.advanced")}</QuietDisclosureTrigger>
      </DisclosureHeading>
      {/* Outside the folded body, and only while the section is open. The body stays mounted while
          folded, so a region inside it would be past its first write delay by the time the section
          opens again, and the note kept from before would be written in the very render that shows
          the body: text that appears with its region, which VoiceOver does not say. Mounted with
          each opening, the region writes the note after its delay, with the trigger still focused. */}
      {expanded && <StatusAnnouncer text={status} />}
      {/* `FlushDisclosureContent` lands the content (the pickers, or the frame around the tabs) in
          line with the fields above, inside the dialog body's 4px of slack. */}
      <FlushDisclosureContent className="flex flex-col gap-4">
        {children}
      </FlushDisclosureContent>
    </Disclosure>
  );
}

/** One kind of setting under `AdvancedTabs`: its tab's id and name, and the fields it holds. */
export type AdvancedTab = { id: string; label: string; content: React.ReactNode };

/** The kinds of setting of an Advanced section, each under a tab of its own, framed together with the row of tabs
 * above them; the first is selected to begin with. A single kind still stands under its tab, so the section looks the
 * same whether one kind or several are on offer. Only the selected panel is mounted; the one shown last is kept while
 * the section folds away, as the section's body stays mounted. */
export function AdvancedTabs({ tabs }: { tabs: AdvancedTab[] }): React.ReactElement {
  const t = useT();
  return (
    <div className="rounded-xl border border-separator p-4">
      <Tabs defaultSelectedKey={tabs[0]?.id} className="gap-4">
        <TextTabStrip label={t("dialog.agentSettings.advanced")} tabs={tabs} />
        {tabs.map((tab) => (
          <TabPanel key={tab.id} id={tab.id} className="mt-0 flex flex-col gap-4 p-0">{tab.content}</TabPanel>
        ))}
      </Tabs>
    </div>
  );
}
