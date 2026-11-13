import React from "react";

import { useT } from "../../i18n/react";
import { MenuFilter } from "../../primitives";
import { FieldClearButton } from "../controls/FieldClearButton";
import type { ActionMenuEntry } from "./menuEntries";
import { MenuItems } from "./MenuItems";

/** A field at the top of a menu that narrows its items to those whose label contains what is typed. */
export interface ActionMenuFilter {
  /** The field's accessible name. */
  label: string;
  /** What the menu says while no item matches. */
  emptyMessage: string;
}

/** The items of a menu with a filter field (`MenuFilter` draws the field and holds what the departures from the
 * standard rest on). `fieldMode` is how the field takes focus as the menu opens. */
export function FilteredMenuItems({ entries, filter, query, onQueryChange, fieldMode, autoFocus }: {
  entries: ActionMenuEntry[];
  filter: ActionMenuFilter;
  query: string;
  onQueryChange: (query: string) => void;
  fieldMode: "typing" | "browsing";
  autoFocus?: "first" | "menu";
}): React.ReactElement {
  const t = useT();
  return (
    <MenuFilter query={query} onQueryChange={onQueryChange} label={filter.label} placeholder={t("common.search")} clearButton={<FieldClearButton />} fieldMode={fieldMode}>
      <MenuItems entries={entries} autoFocus={autoFocus} noMatch={{ query, message: filter.emptyMessage }} />
    </MenuFilter>
  );
}
