import React, { useRef } from "react";

import { MarkLineBox } from "../components/status/MarkLineBox";
import { Switch } from "../primitives";
import type { SettingsSearchId } from "./settingsSearch";
import { useSettingsSearchFilter } from "./settingsSearchFilter";

/** One setting: its name and, when it has one, a line saying what it does at the start, its control
 * at the end, with a thin divider under every row but the last (the copy of a row a list motion is
 * still removing does not count, so the divider goes as the row leaves). At phone widths a wide
 * control moves under the text instead, since beside it it squeezes the description to a few words
 * per line, end-aligned so it sits against the same edge the control has beside the text. Pass
 * `compact` when the control is one small control of a bounded width — a switch, one button, one
 * badge, a link of a word or two, with a status mark beside it at most — so it stays beside the
 * text at every width, where under it it would leave a line almost empty. Leave it off for a
 * select, a text field, a group of buttons or a value whose width varies (a URL). On a row without
 * `compact`, pass `fill` when the control is an input (a select, a text field, a path picker with its Browse
 * button): under the text it then spans the row and its one child grows to fill it, whatever width
 * the child has beside the text; a child that holds buttons next to the input is a flex row whose
 * input has `flex-1` and whose buttons keep their size. Above the tablet boundary `fill` changes
 * nothing. `help` is a help tip that ends the name's line (see "Explanations in help tips" in
 * `docs/product/labels-and-tooltips.md`). `below` is what the row has to say under itself across
 * its whole width (a progress bar, a full-width field such as a tags input), inside its divider. `searchId` is the
 * row's entry in `SETTINGS_SEARCH_INDEX`: the row is hidden, not unmounted, while a search in Settings leaves it out,
 * and a hidden row after it does not keep the last shown row's divider. `motionKey` names the row for a list that
 * animates the rows around it (`useListMotion`); without one, a row with a `searchId` is named by it, so that it comes
 * and goes with the list motion of the Settings section it is in as a search hides and shows it. The description is
 * marked `data-setting-description`, by which `settingsSearch.test.tsx` holds it to its entry's wordings. */
export function SettingRow({
  searchId,
  label,
  help,
  description,
  children,
  below,
  motionKey,
  compact = false,
  fill = false,
}: {
  searchId?: SettingsSearchId;
  label: React.ReactNode;
  help?: React.ReactNode;
  description?: React.ReactNode;
  children?: React.ReactNode;
  below?: React.ReactNode;
  motionKey?: string;
  compact?: boolean;
  fill?: boolean;
}): React.ReactElement {
  const { shows } = useSettingsSearchFilter();
  return (
    <div
      data-list-motion={motionKey ?? searchId}
      data-settings-search-id={searchId}
      hidden={searchId !== undefined && !shows(searchId)}
      className="flex flex-col gap-3 groove-b py-4 [&:not(:has(~:not([data-list-motion-exit],[hidden])))]:border-b-0"
    >
      <div className={compact ? "flex items-center justify-between gap-4 tablet:gap-6" : "flex flex-col items-start gap-3 tablet:flex-row tablet:items-center tablet:justify-between tablet:gap-6"}>
        <div className="min-w-0">
          <div className="flex items-start gap-2 text-sm font-medium text-foreground">
            <div className="min-w-0">{label}</div>
            {help && <MarkLineBox size="text-sm">{help}</MarkLineBox>}
          </div>
          {description && <div data-setting-description="" className="text-sm text-app-muted">{description}</div>}
        </div>
        {/* A row whose controls all render nothing (`false`) draws no empty box taking the gap. */}
        {React.Children.toArray(children).length > 0 && <div className={compact ? "shrink-0" : fill ? "shrink-0 max-tablet:flex max-tablet:w-full max-tablet:*:min-w-0 max-tablet:*:flex-1" : "shrink-0 max-tablet:self-end-safe"}>{children}</div>}
      </div>
      {below}
    </div>
  );
}

/** What a switch's `onChange` may hand back: the save it started, while which further presses are ignored. */
type SwitchChange = (value: boolean) => void | Promise<unknown>;

/** An on/off switch named by `label`, which the setting shows itself. It shows `isSelected`, the value the setting
 * holds, never a value of its own set on press. When `onChange` returns the save it started, a press while that save is
 * unanswered is ignored. The pending save is held in a ref rather than shown: a shown value would be cleared by the
 * reply, which can arrive before the broadcast that updates `isSelected`, so the switch would briefly show the old
 * value again. */
export function SettingSwitch({
  label,
  isSelected,
  onChange,
  describedBy,
  isUnavailable,
  opensFocused,
}: {
  label: string;
  isSelected: boolean;
  onChange: SwitchChange;
  /** The ids of the notes that describe the switch. */
  describedBy?: string;
  /** The switch cannot be changed now: for a reason `describedBy` gives, or only for as long as an operation it
   * started is unanswered, which needs no note. It is not natively disabled, which would take
   * it out of the Tab order and drop focus from a user who is on it: it stays focusable, reads as `aria-disabled`,
   * looks faded, and a press or Space leaves it as it is. */
  isUnavailable?: boolean;
  /** The switch is where the dialog around it puts focus on opening (see `openingFocus`). */
  opensFocused?: boolean;
}): React.ReactElement {
  const saving = useRef(false);
  const change = (value: boolean) => {
    if (isUnavailable || saving.current) return;
    const save = onChange(value);
    if (!save) return;
    saving.current = true;
    // The owner reports a failure; this only frees the switch once the save settles either way.
    const release = () => { saving.current = false; };
    void save.then(release, release);
  };
  return (
    <Switch
      label={label}
      describedBy={describedBy}
      unavailable={isUnavailable}
      opensFocused={opensFocused}
      selected={isSelected}
      onSelectedChange={change}
      className={isUnavailable ? "opacity-(--disabled-opacity)" : undefined}
    />
  );
}

/** A `SettingRow` whose control is an on/off switch. The switch is named by the visible label. */
export function SwitchSettingRow({
  searchId,
  label,
  help,
  description,
  isSelected,
  onChange,
  motionKey,
}: {
  searchId?: SettingsSearchId;
  label: string;
  help?: React.ReactNode;
  description: React.ReactNode;
  isSelected: boolean;
  onChange: SwitchChange;
  motionKey?: string;
}): React.ReactElement {
  return (
    <SettingRow searchId={searchId} label={label} help={help} description={description} motionKey={motionKey} compact>
      <SettingSwitch label={label} isSelected={isSelected} onChange={onChange} />
    </SettingRow>
  );
}
