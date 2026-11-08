import React from "react";

import { OptionSelect } from "../components/form/OptionSelect";
import type { Translate } from "../i18n/catalog";
import { useT } from "../i18n/react";
import {
  ACTIVE_PROJECT_SORTS,
  type ActiveProjectSort,
  INACTIVE_PROJECT_SORTS,
  type InactiveProjectSort,
  type SessionOrder,
  SESSION_SORTS,
  type SessionSort,
} from "../sessions/order";
import { sessionOrderPreference } from "../store";
import { SettingRow } from "./SettingRow";

type SortKey = SessionSort | ActiveProjectSort | InactiveProjectSort;

function sortLabel(t: Translate, sort: SortKey): string {
  switch (sort) {
    case "status": return t("settings.sort.option.status");
    case "started": return t("settings.sort.option.started");
    case "name": return t("settings.sort.option.name");
    case "used": return t("settings.sort.option.used");
  }
}

/** One part of the session order, as a row of its choices (`sorts`, in the order they are offered). */
function SortRow<S extends SortKey>({ label, description, sorts, value, onChange }: {
  label: string;
  description: string;
  sorts: readonly S[];
  value: S;
  onChange: (sort: S) => void;
}): React.ReactElement {
  const t = useT();
  const options = sorts.map((sort) => ({ value: sort, label: sortLabel(t, sort) }));
  return (
    <SettingRow fill label={label} description={description}>
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
      <SortRow label={t("settings.sort.sessions.label")} description={t("settings.sort.sessions.description")} sorts={SESSION_SORTS} value={order.sessions} onChange={choose("sessions")} />
      <SortRow label={t("settings.sort.activeProjects.label")} description={t("settings.sort.activeProjects.description")} sorts={ACTIVE_PROJECT_SORTS} value={order.activeProjects} onChange={choose("activeProjects")} />
      <SortRow label={t("settings.sort.inactiveProjects.label")} description={t("settings.sort.inactiveProjects.description")} sorts={INACTIVE_PROJECT_SORTS} value={order.inactiveProjects} onChange={choose("inactiveProjects")} />
    </>
  );
}
