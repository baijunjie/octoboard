import { expect, it } from "vitest";

import { DEFAULT_ACCOUNT_ONLY_PROVIDER, FIXTURE_PROVIDERS, NO_QUOTA_PROVIDER, UNPRESENTED_PROVIDER } from "../fixtures/fixtureProviders";
import { hasPlanQuota, offersLoginFlow, offersOwnedAccounts, takesMessageImages } from "./providers";

it("offers owned accounts for a served provider that declares them, and not for one that declares it has the default account only", () => {
  const served = [...FIXTURE_PROVIDERS, DEFAULT_ACCOUNT_ONLY_PROVIDER];
  for (const provider of FIXTURE_PROVIDERS) expect(offersOwnedAccounts(served, provider.id), provider.id).toBe(provider.capabilities.owned_accounts);
  expect(DEFAULT_ACCOUNT_ONLY_PROVIDER.capabilities.owned_accounts).toBe(false);
  expect(offersOwnedAccounts(served, DEFAULT_ACCOUNT_ONLY_PROVIDER.id)).toBe(false);
});

it("follows the served capability, not the provider's id", () => {
  const [first] = FIXTURE_PROVIDERS;
  const flipped = { ...first, capabilities: { ...first.capabilities, owned_accounts: !first.capabilities.owned_accounts } };
  expect(offersOwnedAccounts([flipped], first.id)).toBe(flipped.capabilities.owned_accounts);
  expect(offersOwnedAccounts([first], first.id)).toBe(first.capabilities.owned_accounts);
});

it("reads a served provider whose capabilities lack the field, as an older daemon serves it, as offering owned accounts", () => {
  const [first] = FIXTURE_PROVIDERS;
  const { owned_accounts: _omitted, ...older } = first.capabilities;
  expect(offersOwnedAccounts([{ ...first, capabilities: older as typeof first.capabilities }], first.id)).toBe(true);
});

it("offers none for a provider the daemon does not serve, or before it serves any", () => {
  expect(offersOwnedAccounts(FIXTURE_PROVIDERS, "not_served")).toBe(false);
  expect(offersOwnedAccounts([], FIXTURE_PROVIDERS[0].id)).toBe(false);
});

it("has a plan quota exactly when the served provider declares a quota window kind, whatever its id", () => {
  const served = [...FIXTURE_PROVIDERS, NO_QUOTA_PROVIDER];
  for (const provider of FIXTURE_PROVIDERS) expect(hasPlanQuota(served, provider.id), provider.id).toBe(provider.quota_windows.kinds.length > 0);
  expect(NO_QUOTA_PROVIDER.quota_windows.kinds).toEqual([]);
  expect(hasPlanQuota(served, NO_QUOTA_PROVIDER.id)).toBe(false);
  const [first] = FIXTURE_PROVIDERS;
  expect(hasPlanQuota([{ ...first, quota_windows: { ...first.quota_windows, kinds: [] } }], first.id)).toBe(false);
  expect(hasPlanQuota([{ ...first, quota_windows: { ...first.quota_windows, kinds: ["weekly_model"] } }], first.id)).toBe(true);
});

it("reads a provider the daemon does not serve, or before it serves any, as having a plan quota", () => {
  expect(hasPlanQuota(FIXTURE_PROVIDERS, "not_served")).toBe(true);
  expect(hasPlanQuota([], FIXTURE_PROVIDERS[0].id)).toBe(true);
});

it("offers a login flow for a served provider that declares one, and withholds it only from one that explicitly declares none", () => {
  const served = [...FIXTURE_PROVIDERS, NO_QUOTA_PROVIDER];
  for (const provider of FIXTURE_PROVIDERS) expect(offersLoginFlow(served, provider.id), provider.id).toBe(provider.capabilities.login_flow);
  expect(NO_QUOTA_PROVIDER.capabilities.login_flow).toBe(false);
  expect(offersLoginFlow(served, NO_QUOTA_PROVIDER.id)).toBe(false);
});

it("reads a served provider whose capabilities lack login_flow, as an older daemon serves it, as offering a login flow", () => {
  const [first] = FIXTURE_PROVIDERS;
  const { login_flow: _omitted, ...older } = first.capabilities;
  expect(offersLoginFlow([{ ...first, capabilities: older as typeof first.capabilities }], first.id)).toBe(true);
});

it("offers no login flow for a provider the daemon does not serve, or before it serves any", () => {
  expect(offersLoginFlow(FIXTURE_PROVIDERS, "not_served")).toBe(false);
  expect(offersLoginFlow([], FIXTURE_PROVIDERS[0].id)).toBe(false);
});

it("takes images for a served provider that declares them, and not for one that declares it takes none", () => {
  const [first] = FIXTURE_PROVIDERS;
  const taking = { ...first, capabilities: { ...first.capabilities, message_images: true } };
  expect(UNPRESENTED_PROVIDER.capabilities.message_images).toBe(false);
  expect(takesMessageImages([taking, UNPRESENTED_PROVIDER], taking.id)).toBe(true);
  expect(takesMessageImages([taking, UNPRESENTED_PROVIDER], UNPRESENTED_PROVIDER.id)).toBe(false);
  expect(takesMessageImages([{ ...taking, capabilities: { ...taking.capabilities, message_images: false } }], taking.id)).toBe(false);
});

it("reads a served provider whose capabilities lack message_images, as an older daemon serves it, as taking images", () => {
  const [first] = FIXTURE_PROVIDERS;
  const { message_images: _omitted, ...older } = first.capabilities;
  expect(takesMessageImages([{ ...first, capabilities: older as typeof first.capabilities }], first.id)).toBe(true);
});

it("takes no images for a provider the daemon does not serve, or before it serves any", () => {
  expect(takesMessageImages(FIXTURE_PROVIDERS, "not_served")).toBe(false);
  expect(takesMessageImages([], FIXTURE_PROVIDERS[0].id)).toBe(false);
});
