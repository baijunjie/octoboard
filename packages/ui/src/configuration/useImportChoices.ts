import { useState } from "react";

import { isKnownAgent } from "../agents/providers";
import type { Translate } from "../i18n/catalog";
import { useT } from "../i18n/react";
import type { Agent, InspectedConsole, ProviderInfo, SettingsSection } from "../protocol";
import { useProviders } from "../store";
import { chosen, nothingToImport, takesDocumentCloneDir, withCloneDirOffers, type ImportPart, type Inspection } from "./importPlan";
import { providerSettingLines } from "./importValues";

/** The parts checked on opening a review: the settings change how this machine behaves rather than add something to it,
 * so they start unchecked. */
const DEFAULT_CHECKED: Record<ImportPart, boolean> = {
  consoles: true,
  settings: false,
  accounts: true,
};

/** The Settings sections checked on opening a review: every one, since turning the Settings on takes all the sections
 * the file carries unless the user unchecks one. */
const DEFAULT_SECTIONS_CHECKED: Record<SettingsSection, boolean> = {
  general: true,
  app_behavior: true,
  git: true,
  agent_behavior: true,
  keyboard_shortcuts: true,
  cleanup: true,
};

/** `DEFAULT_CHECKED` for `inspection`, except that an accounts region bringing nothing for any agent (one that carries
 * only the three Agent accounts switches, as a file of an earlier version may) starts unchecked like the settings. What
 * it brings is an agent to add, an account, a default account's name or switch that applies, or provider settings or
 * tags of an agent this daemon serves: each shows in that agent's card, where it can be left out. Provider settings
 * count as the card words them (see `providerSettingLines`), so a setting the provider does not export, or a value it
 * does not take, brings nothing. */
function checkedOnOpening(t: Translate, providers: readonly ProviderInfo[], inspection: Inspection): Record<ImportPart, boolean> {
  const { accounts } = inspection;
  const settings = accounts?.agent_settings;
  const servedWithSettings = (Object.entries(settings?.provider_settings ?? {}) as [Agent, Record<string, boolean | string> | undefined][]).some(
    ([agent, wanted]) => providerSettingLines(t, providers, agent, wanted).length > 0,
  );
  const adds =
    !!accounts &&
    (accounts.agents_to_add.length > 0 ||
      accounts.agents.some((agent) => agent.accounts.length > 0 || (agent.default_name_applies && !!agent.default_name) || agent.default_disabled_applies) ||
      servedWithSettings ||
      Object.entries(settings?.agent_tags ?? {}).some(([agent, tags]) => tags !== undefined && isKnownAgent(providers, agent)));
  return { ...DEFAULT_CHECKED, accounts: adds };
}

/** What the user has chosen to import, apart from the per-project and per-account rows. */
interface Basis {
  checked: Record<ImportPart, boolean>;
  sectionsChecked: Record<SettingsSection, boolean>;
  useCloneDir: boolean;
}

/**
 * The review's choices of what is imported: which parts of the file are checked (an unchecked one has no tab and is not
 * sent; read only for a file that carries more than one), which of its Settings sections are while the Settings are, and
 * whether the file's default clone directory is applied with the Git settings or this machine's own is kept. Of
 * `inspection` (absent outside the review) it works out the `parts` and `sections` the choices import, whether there is
 * `nothingChosen`, and the consoles as the review `offered` them, under the file's default clone directory while it is
 * applied. `change` takes some of the choices anew and tells `onOffers` the offers they come to, for the rows to move to;
 * `reset` returns them to what a review opens with.
 */
export function useImportChoices(inspection: Inspection | undefined, onOffers: (offers: InspectedConsole[]) => void) {
  const t = useT();
  const providers = useProviders();
  const [checked, setChecked] = useState(DEFAULT_CHECKED);
  const [sectionsChecked, setSectionsChecked] = useState(DEFAULT_SECTIONS_CHECKED);
  const [useCloneDir, setUseCloneDir] = useState(true);
  const { parts, sections } = inspection ? chosen(inspection, checked, sectionsChecked) : { parts: [], sections: [] };
  const offered = inspection ? withCloneDirOffers(inspection.consoles, takesDocumentCloneDir(inspection, checked, sectionsChecked, useCloneDir)) : [];
  const change = (next: Partial<Basis>) => {
    const basis = { checked, sectionsChecked, useCloneDir, ...next };
    if (inspection) onOffers(withCloneDirOffers(inspection.consoles, takesDocumentCloneDir(inspection, basis.checked, basis.sectionsChecked, basis.useCloneDir)));
    setChecked(basis.checked);
    setSectionsChecked(basis.sectionsChecked);
    setUseCloneDir(basis.useCloneDir);
  };
  const reset = (opened: Inspection) => {
    setChecked(checkedOnOpening(t, providers, opened));
    setSectionsChecked(DEFAULT_SECTIONS_CHECKED);
    setUseCloneDir(true);
  };
  return { checked, sectionsChecked, useCloneDir, parts, sections, nothingChosen: !!inspection && nothingToImport(parts, sections), offered, change, reset };
}
