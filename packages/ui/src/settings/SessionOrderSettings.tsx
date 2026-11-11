import React from "react";

import { OptionSelect } from "../components/form/OptionSelect";
import { useT } from "../i18n/react";
import { ACTIVE_PROJECT_SORTS, INACTIVE_PROJECT_SORTS, type SessionOrder, SESSION_SORTS } from "../sessions/order";
import { SESSION_ORDER_WORDS, sortLabel, type SortKey } from "../sessions/orderWording";
import { sessionOrderPreference } from "../store";
import { SettingRow } from "./SettingRow";
import type { SettingsSearchId } from "./settingsSearch";

/** One part of the session order, as a row of its choices (`sorts`, in the order they are offered). */
function SortRow<S extends SortKey>({ searchId, label, description, sorts, value, onChange }: {
  searchId: SettingsSearchId;
  label: string;
  description: string;
  sorts: readonly S[];
  value: S;
  onChange: (sort: S) => void;
}): React.ReactElement {
  const t = useT();
  const options = sorts.map((sort) => ({ value: sort, label: sortLabel(t, sort) }));
  return (
    <SettingRow fill searchId={searchId} label={label} description={description}>
      <OptionSelect inline label={label} options={options} value={value} onChange={onChange} />
    </SettingRow>
  );
}

/** How sessions and projects are ordered (`SessionOrder`), as three rows: the sessions, the projects with an
 * unarchived session (which come first) and the projects without one (which come last). */
export function SessionOrderSettings(): React.ReactElement {
  const t = useT();
  const order = sessionOrderPreference.useValue();
  const choose = <K extends keyof SessionOrder>(part: K) => (sort: SessionOrder[K]) => sessionOrderPreference.set({ ...sessionOrderPreference.get(), [part]: sort });
  return (
    <>
      <SortRow searchId="session-order.sessions" label={t(SESSION_ORDER_WORDS.sessions.label)} description={t(SESSION_ORDER_WORDS.sessions.description)} sorts={SESSION_SORTS} value={order.sessions} onChange={choose("sessions")} />
      <SortRow searchId="session-order.activeProjects" label={t(SESSION_ORDER_WORDS.activeProjects.label)} description={t(SESSION_ORDER_WORDS.activeProjects.description)} sorts={ACTIVE_PROJECT_SORTS} value={order.activeProjects} onChange={choose("activeProjects")} />
      <SortRow searchId="session-order.inactiveProjects" label={t(SESSION_ORDER_WORDS.inactiveProjects.label)} description={t(SESSION_ORDER_WORDS.inactiveProjects.description)} sorts={INACTIVE_PROJECT_SORTS} value={order.inactiveProjects} onChange={choose("inactiveProjects")} />
    </>
  );
}
