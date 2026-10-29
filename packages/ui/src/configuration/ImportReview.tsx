import { Checkbox, Label } from "@heroui/react";
import React, { useState } from "react";

import { InlineReference } from "../components/InlineReference";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { LabelledGroup } from "../dialogs/LabelledGroup";
import type { PlainMessageKey } from "../i18n/catalog";
import { Message, useT } from "../i18n/react";
import {
  accountsNeedAttention,
  carriedRegions,
  consolesNeedAttention,
  importedRegions,
  withEffectiveFallbacks,
  type AccountReviewState,
  type Inspection,
  type ReviewState,
  type shortcutsReview,
} from "./importPlan";
import { ImportAccounts } from "./ImportAccounts";
import { ImportConsoles } from "./ImportConsoles";
import type { Shown } from "./importParts";
import { ImportSettings } from "./ImportSettings";
import { RegionTabs, type Region, type RegionPanel } from "./RegionTabs";

const TAB_LABEL: Record<Region, PlainMessageKey> = {
  consoles: "configuration.import.tab.consoles",
  accounts: "configuration.import.tab.accounts",
  settings: "configuration.import.tab.settings",
};

const REGION_LABEL: Record<Region, PlainMessageKey> = {
  consoles: "configuration.import.include.consoles",
  accounts: "configuration.import.include.accounts",
  settings: "configuration.import.include.settings",
};

/**
 * The step between reading a document and applying it: the sentence naming the file, a checkbox for every region the
 * document carries (none where it carries only one: importing that file is importing that region), then the checked
 * regions as tabs (a single one stands without a tab bar; see `RegionTabs`), so an unchecked region, which is not
 * imported, has no tab. A tab whose panel needs attention carries a warning glyph. Consoles and projects: see
 * `ImportConsoles`. An account reference to an account the import would create (as proposed or in another directory)
 * is no fallback while the accounts are imported, so the consoles' marks follow the Accounts checkbox and each
 * account's choice. Accounts: see `ImportAccounts`. Application settings: see `ImportSettings`. The state of every
 * choice and the selected tab are the caller's, so applying sends what is on screen, and a problem found at apply can
 * bring its tab forward.
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
  tab,
  onTab,
  shortcuts,
  shown,
}: {
  fileName: string;
  inspection: Inspection;
  review: ReviewState;
  onReview: (review: ReviewState) => void;
  accountReview: AccountReviewState;
  onAccountReview: (review: AccountReviewState) => void;
  /** Which regions the user has checked to be imported; read only for the regions the document carries, and only
   * when it carries more than one. */
  checked: Record<Region, boolean>;
  onChecked: (region: Region, on: boolean) => void;
  tab: Region | undefined;
  onTab: (tab: Region) => void;
  /** What the document's keyboard shortcuts would do on this device; `undefined` when it carries none. */
  shortcuts?: ReturnType<typeof shortcutsReview>;
  /** Holds a field's message back until the user has tried to import. */
  shown: Shown;
}): React.ReactElement {
  const t = useT();
  const [announcement, setAnnouncement] = useState<string>();
  const settings = inspection.settings;
  const accounts = inspection.accounts;
  const carried = carriedRegions(inspection);
  const imported = importedRegions(carried, checked);
  const consoles = withEffectiveFallbacks(inspection.consoles, accounts, accountReview, imported.includes("accounts"));
  const contents: Record<Region, { attention: boolean; content: React.ReactNode } | undefined> = {
    consoles: {
      attention: consolesNeedAttention(consoles),
      content: <ImportConsoles consoles={consoles} review={review} onReview={onReview} onAnnounce={setAnnouncement} shown={shown} />,
    },
    accounts: accounts ? {
      attention: accountsNeedAttention(accounts),
      content: <ImportAccounts accounts={accounts} review={accountReview} onReview={onAccountReview} shown={shown} />,
    } : undefined,
    settings: settings ? { attention: false, content: <ImportSettings settings={settings} shortcuts={shortcuts} /> } : undefined,
  };
  const regions: RegionPanel[] = imported.flatMap((id) => {
    const region = contents[id];
    return region ? [{ id, label: t(TAB_LABEL[id]), ...region }] : [];
  });

  return (
    <>
      <StatusAnnouncer text={announcement} />
      <p className="text-sm">
        <Message id="configuration.import.review.intro" params={{ file: <InlineReference monospace wrap>{fileName}</InlineReference> }} />
      </p>
      {carried.length > 1 && (
        <LabelledGroup label={t("configuration.import.include.label")}>
          <div className="flex flex-wrap gap-x-6 gap-y-2">
            {carried.map((region) => (
              <Checkbox key={region} variant="secondary" isSelected={checked[region]} onChange={(on) => onChecked(region, on)}>
                <Checkbox.Content className="min-h-6">
                  <Checkbox.Control>
                    <Checkbox.Indicator />
                  </Checkbox.Control>
                  <Label>{t(REGION_LABEL[region])}</Label>
                </Checkbox.Content>
              </Checkbox>
            ))}
          </div>
        </LabelledGroup>
      )}
      <RegionTabs label={t("configuration.import.tabs.review")} regions={regions} selected={tab} onSelect={onTab} />
    </>
  );
}
