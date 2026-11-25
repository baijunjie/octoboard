import { FIXTURE_AGENTS, FIXTURE_PROVIDERS, NO_QUOTA_PROVIDER } from "../../fixtures/fixtureProviders";
import type { Account, AccountQuota, Agent } from "../../protocol";
import { accountOf, minutesAgo } from "../../fixtures/builders";
import { quotaFailure, quotaOf } from "../../fixtures/quotas";
import type { Scenario } from "../scenario";

/** What the scenarios of a provider that declares no plan quota and no login flow share: the stand-in provider's accounts,
 * their sign-in readings, and the state that serves it. */
export const NO_QUOTA_AGENT = NO_QUOTA_PROVIDER.id;

/** The stand-in's owned accounts: Work, whose reading cannot tell, and Client, signed out. Its default account has no entry. */
export const noQuotaAccounts: Account[] = [
  accountOf("a-terminal-work", NO_QUOTA_AGENT, "Work", "/Users/dev/.terminal-agent-work"),
  accountOf("a-terminal-client", NO_QUOTA_AGENT, "Client", "/Users/dev/.terminal-agent-client"),
];

/** The sign-in readings of those accounts, which have no plan and no windows: the default account signed in under an
 * email, Work with neither a reason nor an email (the reading could not tell) and Client signed out. */
export const noQuotaReadings: AccountQuota[] = [
  quotaOf(NO_QUOTA_AGENT, null, null, [], { email: "dev@octoboard.example", fetched_at: minutesAgo(2) }),
  quotaOf(NO_QUOTA_AGENT, "a-terminal-work", null, []),
  quotaOf(NO_QUOTA_AGENT, "a-terminal-client", null, [], {
    unavailable: quotaFailure("quota_not_signed_in", { agent: NO_QUOTA_PROVIDER.label }, `${NO_QUOTA_PROVIDER.label} is not signed in`),
  }),
];

/** `state` with the stand-in served beside its providers, added and installed, holding its default account and
 * `accounts` with the sign-in `readings`. */
export function withNoQuotaProvider(
  state: NonNullable<Scenario["state"]>,
  { accounts = noQuotaAccounts, readings = noQuotaReadings }: { accounts?: Account[]; readings?: AccountQuota[] } = {},
): NonNullable<Scenario["state"]> {
  const settings = state.settings!;
  return {
    ...state,
    providers: [...(state.providers ?? FIXTURE_PROVIDERS), NO_QUOTA_PROVIDER],
    settings: {
      ...settings,
      added_agents: [...(settings.added_agents ?? FIXTURE_AGENTS), NO_QUOTA_AGENT],
      accounts: [...settings.accounts, ...accounts],
      provider_settings: { ...settings.provider_settings, [NO_QUOTA_AGENT]: {} },
    },
    agentAvailability: new Map(state.agentAvailability).set(NO_QUOTA_AGENT as Agent, { agent: NO_QUOTA_AGENT, availability: "available", default_account_dir: "/Users/dev/.terminal-agent" }),
    accountQuotas: [...(state.accountQuotas ?? []), ...readings],
  };
}
