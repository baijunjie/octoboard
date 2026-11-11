import React, { createContext, useContext } from "react";

import type { PlainMessageKey } from "../i18n/catalog";
import type { SectionMatches, SettingsSearchId } from "./settingsSearch";

/** Which of a section's rows and groups the search leaves on screen: all of them while nothing is searched for. */
export interface SettingsSearchFilter {
  shows: (id: SettingsSearchId) => boolean;
  showsGroup: (group: PlainMessageKey) => boolean;
}

const EVERYTHING: SettingsSearchFilter = { shows: () => true, showsGroup: () => true };

const SettingsSearchFilterContext = createContext<SettingsSearchFilter>(EVERYTHING);

/** The filter of the section the caller is in (see `SettingsSearchProvider`); outside one, everything is shown. */
export function useSettingsSearchFilter(): SettingsSearchFilter {
  return useContext(SettingsSearchFilterContext);
}

/** Puts a section's matches (see `sectionMatches`) in force for the rows and groups inside it, or shows everything
 * while `matches` is undefined, which is while nothing is searched for. */
export function SettingsSearchProvider({ matches, children }: { matches: SectionMatches | undefined; children: React.ReactNode }): React.ReactElement {
  const filter: SettingsSearchFilter = matches ? { shows: (id) => matches.ids.has(id), showsGroup: (group) => matches.groups.has(group) } : EVERYTHING;
  return <SettingsSearchFilterContext.Provider value={filter}>{children}</SettingsSearchFilterContext.Provider>;
}

/** A part of a section that a search finds as a whole rather than row by row (a list of the user's own things), by
 * its entry in `SETTINGS_SEARCH_INDEX`; hidden, not unmounted, while a search does not find it, so its state is kept.
 * It comes and goes as one item of the section's list motion. `data-settings-search-block` tells it from a row. */
export function SettingsSearchBlock({ id, children }: { id: SettingsSearchId; children: React.ReactNode }): React.ReactElement {
  const { shows } = useSettingsSearchFilter();
  return <div data-settings-search-id={id} data-settings-search-block="" data-list-motion={id} hidden={!shows(id)}>{children}</div>;
}
