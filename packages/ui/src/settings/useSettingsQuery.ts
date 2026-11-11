import { useState } from "react";

import { useTrimmedField } from "../components/form/useTrimmedField";
import type { PlainMessageKey } from "../i18n/catalog";
import { useCurrentLanguage, useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { useDaemonStoreOrEmpty } from "../store";
import { searchReads, useSettingsReads } from "./settingsReads";
import type { SettingsSectionId } from "./sections";
import { queryWords, sectionMatches, type SectionMatches, type SettingsSearchContext } from "./settingsSearch";
import { useFollowMacosPermissions } from "./useMacosPermissions";

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
  /** The section on screen: the one kept (opened on, selected, or last fallen back to) while it is listed, else the
   * first listed (the kept one when nothing is listed). */
  shownId: SettingsSectionId;
  /** How many settings match across every section. */
  total: number;
  /** The props of the search field. */
  queryField: ReturnType<typeof useTrimmedField>;
  /** What the dialog's status region says of the search, nothing while nothing is searched for. */
  announcement: string | undefined;
  /** Shows a section, whichever the dialog had on screen. */
  select: (id: SettingsSectionId) => void;
  /** Empties the query. */
  clear: () => void;
}

/** The query of the Settings dialog's search field, matched against `sectionIds` (named by `sectionLabel`) in the current
 * language and in the context that decides which settings each section shows. `initialId` is the section to start on;
 * the hook keeps the one on screen from there, so clearing the field keeps it. */
export function useSettingsQuery({ sectionIds, sectionLabel, initialId }: {
  sectionIds: readonly SettingsSectionId[];
  sectionLabel: (id: SettingsSectionId) => PlainMessageKey;
  initialId: SettingsSectionId;
}): SettingsQuery {
  const t = useT();
  const language = useCurrentLanguage();
  const platform = usePlatform();
  const keepAwake = useDaemonStoreOrEmpty((s) => s.host?.keep_awake === true);
  const settings = useDaemonStoreOrEmpty((s) => s.settings);
  const reads = useSettingsReads();
  const [query, setQuery] = useState("");
  const [currentId, setCurrentId] = useState(initialId);
  // A row that waits on a read of the system is counted from the answer on, in step with the row itself.
  const context: SettingsSearchContext = { platform, keepAwake, settings, reads: searchReads(reads) };
  const words = queryWords(query, language);
  const searching = words.length > 0;
  // The permissions are counted from what they are now, so a search keeps that read up to date.
  useFollowMacosPermissions(searching);
  const matches = new Map<SettingsSectionId, SectionMatches>(searching ? sectionIds.map((id) => [id, sectionMatches(id, sectionLabel(id), words, t, language, context)]) : []);
  const countOf = (id: SettingsSectionId) => matches.get(id)?.ids.size ?? 0;
  const listedIds = searching ? sectionIds.filter((id) => countOf(id) > 0) : [...sectionIds];
  // The section on screen stays while it is listed, and the first listed replaces it only when it is gone, so that a read
  // answering or any other re-match does not move the panel. While nothing is listed it stays as it is, hidden.
  const shownId = listedIds.includes(currentId) ? currentId : listedIds[0] ?? currentId;
  // What is shown is what is kept, so that clearing the query keeps it.
  if (shownId !== currentId) setCurrentId(shownId);
  const total = listedIds.reduce((sum, id) => sum + countOf(id), 0);
  const queryField = useTrimmedField(query, setQuery);
  const announcement = searching ? (total === 0 ? t("settings.search.none.sentence") : t("settings.search.found", { count: total })) : undefined;
  return { query, words, matches, countOf, listedIds, shownId, total, queryField, announcement, select: setCurrentId, clear: () => setQuery("") };
}
