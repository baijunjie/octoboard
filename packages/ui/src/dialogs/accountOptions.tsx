import { sharingChangeUnavailable, type ChoiceEntry } from "../accountChoices";
import type { Translate } from "../i18n/catalog";
import { accountUsage } from "../quota/accountUsage";
import type { Option } from "./OptionSelect";

/** An account's entry in a picker: its name behind a compact usage bar. One whose quota is used up, or whose
 * sharing change runs, is shown, with why in short under its name, but cannot be chosen (see `accountUsage`). */
export function accountOption(t: Translate, entry: ChoiceEntry, now: number, isDisabled: boolean): Option<string> {
  const usage = accountUsage(t, entry.agent, entry.quota, entry.exhaustedBy, now);
  return {
    value: entry.key,
    label: entry.name,
    ...(entry.changingSharing ? { icon: usage.icon, ...sharingChangeUnavailable(t) } : usage),
    isDisabled: isDisabled || entry.exhaustedBy !== undefined || entry.changingSharing,
  };
}
