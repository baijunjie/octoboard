import { accountDisabledPickable, accountDisabledUnavailable, accountSignedOutUnavailable, type ChoiceEntry } from "../accounts/choiceEntries";
import type { ProviderInfo } from "../protocol";
import type { Translate } from "../i18n/catalog";
import { accountUsage } from "../quota/accountUsage";
import type { Option } from "../components/form/OptionSelect";

/** An account's entry in a picker: its name behind a compact usage bar. One that is turned off, is not logged in,
 * has its quota used up, or has a sharing change running is shown but cannot be chosen. The first two say why in short
 * under its name and in full as its description (`accountDisabledUnavailable`, `accountSignedOutUnavailable`), in place
 * of what remains of its quota; a used-up one says so through its usage (see `accountUsage`); a sharing change, which
 * ends on its own, says nothing. */
export function accountOption(t: Translate, providers: readonly ProviderInfo[], entry: ChoiceEntry, now: number, isDisabled: boolean): Option<string> {
  const usage = accountUsage(t, providers, entry.agent, entry.quota, entry.exhaustedBy, now);
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

/** The options of a picker that keeps a turned-off account selectable (a console's or a project's account): a turned-off
 * account (`off`) is left out, but for those whose `value` is one of `kept` (the account picked now, and the one saved
 * when the dialog opened, so the user can pick it back), which stay marked (`markDisabledPickable`). */
export function offeredAccountOptions(t: Translate, entries: { value: string; label: string; off: boolean }[], ...kept: string[]): Option<string>[] {
  return entries
    .filter(({ value, off }) => !off || kept.includes(value))
    .map(({ value, label, off }) => markDisabledPickable(t, { value, label }, off));
}
