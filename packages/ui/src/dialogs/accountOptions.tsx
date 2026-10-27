import { accountDisabledPickable, accountDisabledUnavailable, accountSignedOutUnavailable, type ChoiceEntry } from "../accountChoices";
import type { Translate } from "../i18n/catalog";
import { accountUsage } from "../quota/accountUsage";
import type { Option } from "./OptionSelect";

/** An account's entry in a picker: its name behind a compact usage bar. One that is turned off, is not logged in,
 * has its quota used up, or has a sharing change running is shown but cannot be chosen. The first two say why in short
 * under its name and in full as its description (`accountDisabledUnavailable`, `accountSignedOutUnavailable`), in place
 * of what remains of its quota; a used-up one says so through its usage (see `accountUsage`); a sharing change, which
 * ends on its own, says nothing. */
export function accountOption(t: Translate, entry: ChoiceEntry, now: number, isDisabled: boolean): Option<string> {
  const usage = accountUsage(t, entry.agent, entry.quota, entry.exhaustedBy, now);
  return {
    value: entry.key,
    label: entry.name,
    ...(entry.disabled ? { icon: usage.icon, ...accountDisabledUnavailable(t) } : entry.signedOut ? { icon: usage.icon, ...accountSignedOutUnavailable(t) } : usage),
    isDisabled: isDisabled || entry.exhaustedBy !== undefined || entry.changingSharing || entry.signedOut || entry.disabled,
  };
}

/** `option` of a picker that keeps a turned-off account selectable (a console's or a project's account), marked
 * "Disabled" when `disabled` (see `accountDisabledPickable`). */
export function markDisabledPickable<O extends { value: string; label: string }>(t: Translate, option: O, disabled: boolean): O | (O & ReturnType<typeof accountDisabledPickable>) {
  return disabled ? { ...option, ...accountDisabledPickable(t) } : option;
}
