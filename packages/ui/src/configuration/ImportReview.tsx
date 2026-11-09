import React, { useState } from "react";

import { InlineReference } from "../i18n/InlineReference";
import { StatusAnnouncer } from "../components/announce/StatusAnnouncer";
import { LabelledGroup } from "../components/form/LabelledGroup";
import type { PlainMessageKey } from "../i18n/catalog";
import { Message, useT } from "../i18n/react";
import {
  accountsNeedAttention,
  carriedParts,
  consolesNeedAttention,
  importedParts,
  regionsOf,
  withEffectiveFallbacks,
  type AccountReviewState,
  type ImportPart,
  type Inspection,
  type ReviewState,
  type shortcutsReview,
} from "./importPlan";
import { ImportAccounts } from "./ImportAccounts";
import { ImportConsoles } from "./ImportConsoles";
import type { ImportChoice, Shown } from "./importParts";
import type { ClientSettingValue } from "./clientSettings";
import type { SettingValue } from "./importValues";
import { ImportSettings } from "./ImportSettings";
import { RegionTabs, type Region, type RegionPanel } from "./RegionTabs";
import type { SettingsSection } from "../protocol";
import { SETTINGS_SECTIONS } from "./settingsSections";
import { Checkbox } from "../primitives";

/** The name of each part of a document, as its checkbox and its tab read. */
const PART_LABEL: Record<ImportPart, PlainMessageKey> = {
  consoles: "configuration.import.tab.consoles",
  settings: "settings.title",
  accounts: "settings.section.accounts",
};

/**
 * The step between reading a document and applying it: the sentence naming the file, a checkbox for every part the
 * document carries (the consoles and projects, the Settings, the accounts; none where it carries only one: importing
 * that file is importing that part), then the regions of the checked parts as tabs, always with the tab bar, even for a
 * single one (see `RegionTabs`), so an unchecked part, which is not imported, has no tab. The Settings tab stands while
 * the Settings are checked, with a card and checkbox for each carried section. A tab whose panel needs attention
 * carries a warning glyph. Consoles and projects: see `ImportConsoles`. An account reference to an account the import
 * would create (as proposed or in another directory) is no fallback while the accounts are imported, so the consoles'
 * marks follow the Agent accounts checkbox and each account's choice. Agent accounts: see `ImportAccounts`. Settings:
 * see `ImportSettings`. The state of every choice and the selected tab are the caller's, so applying sends what is on
 * screen, and a problem found at apply can bring its tab forward.
 */
export function ImportReview({
  fileName,
  inspection,
  review,
  onReview,
  accountReview,
  onAccountReview,
  checked,
  onChecked,
  sectionsChecked,
  onSectionChecked,
  tab,
  onTab,
  shortcuts,
  clientSettings,
  sorting,
  cloneDir,
  shown,
}: {
  fileName: string;
  inspection: Inspection;
  review: ReviewState;
  onReview: (review: ReviewState) => void;
  accountReview: AccountReviewState;
  onAccountReview: (review: AccountReviewState) => void;
  /** Which parts the user has checked to be imported; read only for the parts the document carries, and only when it
   * carries more than one. */
  checked: Record<ImportPart, boolean>;
  onChecked: (part: ImportPart, on: boolean) => void;
  /** Which of the document's Settings sections the user has checked to be imported; read only for the sections the
   * document carries, and only while the Settings are imported. */
  sectionsChecked: Record<SettingsSection, boolean>;
  onSectionChecked: (section: SettingsSection, on: boolean) => void;
  tab: Region | undefined;
  onTab: (tab: Region) => void;
  /** What the document's keyboard shortcuts would do on this device; `undefined` when it carries none. */
  shortcuts?: ReturnType<typeof shortcutsReview>;
  /** What the settings only this client holds, and the language the daemon holds, would do here, one for each the
   * document carries. */
  clientSettings: ClientSettingValue[];
  /** The items of the session order the document carries that would change; `undefined` when it carries none. */
  sorting?: SettingValue[];
  /** Whether the file's default clone directory is applied with the Git settings, and the change of that. */
  cloneDir: { use: boolean; onUse: (use: boolean) => void };
  /** Holds a field's message back until the user has tried to import. */
  shown: Shown;
}): React.ReactElement {
  const t = useT();
  const [announcement, setAnnouncement] = useState<string>();
  const settings = inspection.settings;
  const accounts = inspection.accounts;
  const carried = carriedParts(inspection);
  const imported = importedParts(carried, checked);
  const consoles = withEffectiveFallbacks(inspection.consoles, accounts, accountReview, imported.includes("accounts"));
  const choices = Object.fromEntries(SETTINGS_SECTIONS.map((section) => [section, { on: sectionsChecked[section], onChange: (on: boolean) => onSectionChecked(section, on) }])) as Record<SettingsSection, ImportChoice>;
  const contents: Record<Region, { attention: boolean; content: React.ReactNode } | undefined> = {
    consoles: {
      attention: consolesNeedAttention(consoles),
      content: <ImportConsoles consoles={consoles} review={review} onReview={onReview} onAnnounce={setAnnouncement} shown={shown} />,
    },
    settings: settings ? { attention: false, content: <ImportSettings settings={settings} sections={inspection.settings_sections} choices={choices} clientSettings={clientSettings} sorting={sorting} shortcuts={shortcuts} cloneDir={cloneDir} /> } : undefined,
    accounts: accounts ? {
      attention: accountsNeedAttention(accounts, accountReview),
      content: <ImportAccounts accounts={accounts} review={accountReview} onReview={onAccountReview} shown={shown} />,
    } : undefined,
  };
  const regions: RegionPanel[] = regionsOf(imported).flatMap((id) => {
    const region = contents[id];
    return region ? [{ id, label: t(PART_LABEL[id]), ...region }] : [];
  });

  return (
    <>
      <StatusAnnouncer text={announcement} />
      <p className="text-sm">
        <Message id="configuration.import.review.intro" params={{ file: <InlineReference monospace wrap>{fileName}</InlineReference> }} />
      </p>
      {carried.length > 1 && (
        <LabelledGroup label={t("configuration.import.include.label")}>
          {/* 16px from the label, as the export dialog spaces its groups. */}
          <div className="mt-2.5 flex flex-wrap gap-x-6 gap-y-2">
            {carried.map((part) => (
              <Checkbox key={part} variant="secondary" contentClassName="min-h-6" selected={checked[part]} onSelectedChange={(on) => onChecked(part, on)}>
                {t(PART_LABEL[part])}
              </Checkbox>
            ))}
          </div>
        </LabelledGroup>
      )}
      <RegionTabs label={t("configuration.import.tabs.review")} regions={regions} selected={tab} onSelect={onTab} alwaysTabs />
    </>
  );
}
