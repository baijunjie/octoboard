import { useState } from "react";

import { useTrimmedField } from "../components/form/useTrimmedField";
import type { PlainMessageKey } from "../i18n/catalog";
import { useCurrentLanguage, useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { useDaemonStoreOrEmpty } from "../store";
import type { SettingsSectionId } from "./sections";
import { queryWords, sectionMatches, type SectionMatches, type SettingsSearchContext } from "./settingsSearch";

/** The search of the Settings dialog, worked out for the query in its field. */
export interface SettingsQuery {
  query: string;
  /** The query's words (see `queryWords`): none while nothing is searched for. */
  words: string[];
  /** Each listed section's matches while something is searched for; empty otherwise. */
  matches: ReadonlyMap<SettingsSectionId, SectionMatches>;
  /** How many settings of `id` match. */
  countOf: (id: SettingsSectionId) => number;
  /** The sections to list: those with a match while something is searched for, every one otherwise. */
  listedIds: SettingsSectionId[];
  /** The section on screen: the selected one, or the first listed while the search finds nothing in it. */
  shownId: SettingsSectionId;
  /** How many settings match across every section. */
  total: number;
  /** The props of the search field. */
  queryField: ReturnType<typeof useTrimmedField>;
  /** What the dialog's status region says of the search, nothing while nothing is searched for. */
  announcement: string | undefined;
  /** Empties the query. */
  clear: () => void;
}

/** The query of the Settings dialog's search field, matched against `sectionIds` (named by `sectionLabel`) in the current
 * language and in the context that decides which settings each section shows. `selectedId` is the section the dialog
 * has selected, and `select` selects one: at each change of the query the section on screen becomes the selected one,
 * so clearing the field keeps it. */
export function useSettingsQuery({ sectionIds, sectionLabel, selectedId, select }: {
  sectionIds: readonly SettingsSectionId[];
  sectionLabel: (id: SettingsSectionId) => PlainMessageKey;
  selectedId: SettingsSectionId;
  select: (id: SettingsSectionId) => void;
}): SettingsQuery {
  const t = useT();
  const language = useCurrentLanguage();
  const platform = usePlatform();
  const keepAwake = useDaemonStoreOrEmpty((s) => s.host?.keep_awake === true);
  const settings = useDaemonStoreOrEmpty((s) => s.settings);
  const [query, setQuery] = useState("");
  const context: SettingsSearchContext = { platform, keepAwake, settings };
  const words = queryWords(query, language);
  const searching = words.length > 0;
  const matches = new Map<SettingsSectionId, SectionMatches>(searching ? sectionIds.map((id) => [id, sectionMatches(id, sectionLabel(id), words, t, language, context)]) : []);
  const countOf = (id: SettingsSectionId) => matches.get(id)?.ids.size ?? 0;
  const listedIds = searching ? sectionIds.filter((id) => countOf(id) > 0) : [...sectionIds];
  const shownId = listedIds.includes(selectedId) ? selectedId : listedIds[0] ?? selectedId;
  const total = listedIds.reduce((sum, id) => sum + countOf(id), 0);
  const queryField = useTrimmedField(query, (next) => {
    select(shownId);
    setQuery(next);
  });
  const announcement = searching ? (total === 0 ? t("settings.search.none.sentence") : t("settings.search.found", { count: total })) : undefined;
  return { query, words, matches, countOf, listedIds, shownId, total, queryField, announcement, clear: () => setQuery("") };
}
