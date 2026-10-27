import { describe, expect, it } from "vitest";

import type { ChoiceEntry } from "../accountChoices";
import { format } from "../i18n/catalog";
import type { Translate } from "../i18n/catalog";
import { accountOption } from "./accountOptions";

const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;
const entry = (extra: Partial<ChoiceEntry> = {}): ChoiceEntry => ({
  key: "claude:w", agent: "claude", account: "w", name: "Work", quota: undefined, exhaustedBy: undefined, changingSharing: false, signedOut: false, disabled: false, ...extra,
});

describe("accountOption", () => {
  it("offers an account that is free", () => {
    const option = accountOption(t, entry(), 0, false);
    expect(option.isDisabled).toBe(false);
    expect(option.reason).toBeUndefined();
  });

  it("lists an account whose sharing change runs as unavailable, without a reason", () => {
    const option = accountOption(t, entry({ changingSharing: true }), 0, false);
    expect(option.isDisabled).toBe(true);
    expect(option.reason).toBeUndefined();
    expect(option.detail).toBeUndefined();
  });

  it("lists a disabled account as unavailable, with why in short and in full, ahead of a running sharing change", () => {
    const option = accountOption(t, entry({ disabled: true, changingSharing: true }), 0, false);
    expect(option.isDisabled).toBe(true);
    expect(option.reason).toEqual({ state: "Disabled" });
    expect(option.detail).toBe("Unavailable while this account is disabled");
  });

  it("lists an account that is not logged in as unavailable, with why in short and in full, ahead of a running sharing change", () => {
    const option = accountOption(t, entry({ signedOut: true, changingSharing: true }), 0, false);
    expect(option.isDisabled).toBe(true);
    expect(option.reason).toEqual({ state: "Not logged in" });
    expect(option.detail).toBe("Unavailable until it is logged in");
  });

  it("gives the used-up quota as the reason when a sharing change runs as well", () => {
    const exhaustedBy = { kind: "session", label: null, used_percent: 100, resets_at: null, window_minutes: 300 } as const;
    const option = accountOption(t, entry({ changingSharing: true, exhaustedBy, quota: { agent: "claude", account: "w", fetched_at: 0, plan: null, unavailable: null, windows: [{ ...exhaustedBy }] } }), 0, false);
    expect(option.isDisabled).toBe(true);
    expect(option.reason).toEqual({ state: "Limit reached" });
  });
});
