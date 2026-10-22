import { describe, expect, it } from "vitest";

import type { ChoiceEntry } from "../accountChoices";
import { format } from "../i18n/catalog";
import type { Translate } from "../i18n/catalog";
import { accountOption } from "./accountOptions";

const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;
const entry = (extra: Partial<ChoiceEntry> = {}): ChoiceEntry => ({
  key: "claude:w", agent: "claude", account: "w", name: "Work", quota: undefined, exhaustedBy: undefined, changingSharing: false, ...extra,
});

describe("accountOption", () => {
  it("offers an account that is free", () => {
    const option = accountOption(t, entry(), 0, false);
    expect(option.isDisabled).toBe(false);
    expect(option.reason).toBeUndefined();
  });

  it("lists an account whose sharing change runs as unavailable, with why in short and in full", () => {
    const option = accountOption(t, entry({ changingSharing: true }), 0, false);
    expect(option.isDisabled).toBe(true);
    expect(option.reason).toEqual({ state: "Changing sharing" });
    expect(option.detail).toBe("Unavailable while its sharing is being changed");
  });

  it("gives the running change as the reason when the quota is used up as well", () => {
    const exhaustedBy = { kind: "session", label: null, used_percent: 100, resets_at: null, window_minutes: 300 } as const;
    const option = accountOption(t, entry({ changingSharing: true, exhaustedBy }), 0, false);
    expect(option.isDisabled).toBe(true);
    expect(option.reason).toEqual({ state: "Changing sharing" });
  });
});
