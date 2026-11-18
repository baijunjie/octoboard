import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { ChoiceEntry } from "../accounts/choiceEntries";
import { format } from "../i18n/catalog";
import type { Translate } from "../i18n/catalog";
import { TEST_PROVIDERS } from "../test/testProviders";
import { accountOption } from "./accountOptions";

const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;
const entry = (extra: Partial<ChoiceEntry> = {}): ChoiceEntry => ({
  key: "claude:w", agent: "claude", account: "w", name: "Work", quota: undefined, exhaustedBy: undefined, changingSharing: false, signedOut: false, disabled: false, ...extra,
});

const exhaustedBy = { kind: "session", label: null, used_percent: 100, resets_at: null, window_minutes: 300 } as const;
const usedUp = (extra: Partial<ChoiceEntry> = {}): ChoiceEntry => entry({
  exhaustedBy, quota: { agent: "claude", account: "w", fetched_at: 0, plan: null, unavailable: null, windows: [{ ...exhaustedBy }], email: null }, ...extra,
});

describe("accountOption", () => {
  it("offers an account that is free", () => {
    const option = accountOption(t, TEST_PROVIDERS, entry(), 0, false, true);
    expect(option.isDisabled).toBe(false);
    expect(option.reason).toBeUndefined();
  });

  it("lists an account whose sharing change runs as unavailable, without a reason", () => {
    const option = accountOption(t, TEST_PROVIDERS, entry({ changingSharing: true }), 0, false, true);
    expect(option.isDisabled).toBe(true);
    expect(option.reason).toBeUndefined();
    expect(option.detail).toBeUndefined();
  });

  it("lists a disabled account as unavailable, with why in short and in full, ahead of a running sharing change", () => {
    const option = accountOption(t, TEST_PROVIDERS, entry({ disabled: true, changingSharing: true }), 0, false, true);
    expect(option.isDisabled).toBe(true);
    expect(option.reason).toEqual({ state: "Disabled" });
    expect(option.detail).toBe("Unavailable while this account is disabled");
  });

  it("lists an account that is not logged in as unavailable, with why in short and in full, ahead of a running sharing change", () => {
    const option = accountOption(t, TEST_PROVIDERS, entry({ signedOut: true, changingSharing: true }), 0, false, true);
    expect(option.isDisabled).toBe(true);
    expect(option.reason).toEqual({ state: "Not logged in" });
    expect(option.detail).toBe("Unavailable until it is logged in");
  });

  it("gives the used-up quota as the reason when a sharing change runs as well", () => {
    const option = accountOption(t, TEST_PROVIDERS, usedUp({ changingSharing: true }), 0, false, true);
    expect(option.isDisabled).toBe(true);
    expect(option.reason).toEqual({ state: "Limit reached" });
  });

  it("lists an account whose quota is used up as unavailable when used-up accounts are refused", () => {
    const option = accountOption(t, TEST_PROVIDERS, usedUp(), 0, false, true);
    expect(option.isDisabled).toBe(true);
    expect(option.reason).toEqual({ state: "Limit reached" });
  });

  it("offers an account whose quota is used up, still saying so, when used-up accounts are not refused", () => {
    const option = accountOption(t, TEST_PROVIDERS, usedUp(), 0, false, false);
    expect(option.isDisabled).toBe(false);
    expect(option.reason).toEqual({ state: "Limit reached" });
  });

  it("keeps an account that is disabled, not logged in or changing its sharing unavailable whether or not used-up accounts are refused", () => {
    for (const extra of [{ disabled: true }, { signedOut: true }, { changingSharing: true }]) {
      for (const refuseUsedUp of [false, true]) {
        expect(accountOption(t, TEST_PROVIDERS, usedUp(extra), 0, false, refuseUsedUp).isDisabled).toBe(true);
      }
    }
  });

  it("draws a used-up option's bar grey whether or not it is refused, and a free one's in the provider's fill", () => {
    const iconOf = (option: ReturnType<typeof accountOption>) => renderToStaticMarkup(option.icon as React.ReactElement);
    for (const refuseUsedUp of [false, true]) {
      expect(iconOf(accountOption(t, TEST_PROVIDERS, usedUp(), 0, false, refuseUsedUp))).toContain("quota-meter--used-up");
    }
    expect(iconOf(accountOption(t, TEST_PROVIDERS, entry(), 0, false, false))).toContain("quota-meter--provider");
  });

  it("disables every account when the picker as a whole is, used up or not", () => {
    for (const refuseUsedUp of [false, true]) {
      expect(accountOption(t, TEST_PROVIDERS, entry(), 0, true, refuseUsedUp).isDisabled).toBe(true);
    }
  });
});
