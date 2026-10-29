import { Disclosure, Tabs } from "@heroui/react";
import { buttonVariants } from "@heroui/styles";
import React from "react";

import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { useTabScrollLabels } from "../components/tabScrollLabels";
import { useT } from "../i18n/react";

/** The collapsed Advanced section of a dialog, folded away by default as what is inside is rarely
 * set. The trigger is drawn as a small ghost button in muted text, by the user's decision, so at
 * rest it reads as an action rather than a field label set in from the others. `ms-0` drops the
 * button's negative icon margin on the label's side of the chevron. */
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
    <Disclosure isExpanded={expanded} onExpandedChange={onExpandedChange}>
      <Disclosure.Heading>
        <Disclosure.Trigger className={`${buttonVariants({ variant: "ghost", size: "sm" })} text-app-muted hover:text-foreground data-[focus-visible=true]:text-foreground`}>
          {t("dialog.agentSettings.advanced")}
          <Disclosure.Indicator className="ms-0" />
        </Disclosure.Trigger>
      </Disclosure.Heading>
      {/* Outside the folded body, and only while the section is open. The body stays mounted while
          folded, so a region inside it would be past its first write delay by the time the section
          opens again, and the note kept from before would be written in the very render that shows
          the body: text that appears with its region, which VoiceOver does not say. Mounted with
          each opening, the region writes the note after its delay, with the trigger still focused. */}
      {expanded && <StatusAnnouncer text={status} />}
      {/* HeroUI pads the body by 8px on every side and the content clips what overflows it, so that
          padding is the room the fields' focus rings need. Taking all of it out as a margin would
          reach past the dialog body, whose own slack is 4px, so the content takes 4px of it back
          and the body's inner element the other 4px, which lands the content (the pickers, or the
          frame around the tabs) in line with the fields above with 2px of ring to spare inside the
          clip. The padding left above the first note and below the last field is HeroUI's own, the
          same on both sides. */}
      <Disclosure.Content className="-mx-1">
        <Disclosure.Body className="-mx-1 flex flex-col gap-4">
          {children}
        </Disclosure.Body>
      </Disclosure.Content>
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
  const tabScrollLabels = useTabScrollLabels();
  return (
    <div className="rounded-xl border border-separator p-4">
      <Tabs defaultSelectedKey={tabs[0]?.id} className="gap-4">
        <Tabs.ListContainer className="w-max max-w-full self-start" render={(props) => <div {...props}>{tabScrollLabels(props.children)}</div>}>
          <Tabs.List aria-label={t("dialog.agentSettings.advanced")} className="w-max min-w-0 p-1.5">
            {tabs.map((tab) => (
              <Tabs.Tab key={tab.id} id={tab.id} className="w-auto flex-none scroll-mx-1.5 whitespace-nowrap hover:text-foreground hover:opacity-100">
                {tab.label}
                <Tabs.Indicator />
              </Tabs.Tab>
            ))}
          </Tabs.List>
        </Tabs.ListContainer>
        {tabs.map((tab) => (
          <Tabs.Panel key={tab.id} id={tab.id} className="mt-0 flex flex-col gap-4 p-0">{tab.content}</Tabs.Panel>
        ))}
      </Tabs>
    </div>
  );
}
