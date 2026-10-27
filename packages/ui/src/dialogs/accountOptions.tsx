import { accountDisabledPickable, accountDisabledUnavailable, type ChoiceEntry } from "../accountChoices";
import type { Translate } from "../i18n/catalog";
import { accountUsage } from "../quota/accountUsage";
import type { Option } from "./OptionSelect";

/** An account's entry in a picker: its name behind a compact usage bar. One whose quota is used up, or whose
 * sharing change runs, is shown but cannot be chosen; the first says why in short under its name (see `accountUsage`). */
export function accountOption(t: Translate, entry: ChoiceEntry, now: number, isDisabled: boolean): Option<string> {
  const usage = accountUsage(t, entry.agent, entry.quota, entry.exhaustedBy, now);
  return {
    value: entry.key,
    label: entry.name,
    ...(entry.disabled ? { icon: usage.icon, ...accountDisabledUnavailable(t) } : usage),
    isDisabled: isDisabled || entry.exhaustedBy !== undefined || entry.changingSharing || entry.disabled,
  };
}

/** `option` of a picker that keeps a turned-off account selectable (a console's or a project's account), marked
 * "Disabled" when `disabled` (see `accountDisabledPickable`). */
export function markDisabledPickable<O extends { value: string; label: string }>(t: Translate, option: O, disabled: boolean): O | (O & ReturnType<typeof accountDisabledPickable>) {
  return disabled ? { ...option, ...accountDisabledPickable(t) } : option;
}
