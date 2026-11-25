import React from "react";

import { offersLoginFlow } from "../../agents/providers";
import { SignInHint } from "../../components/text/SignInHint";
import { HelpTip } from "../../components/hint/HelpTip";
import { isolate } from "../../i18n/isolate";
import { useT } from "../../i18n/react";
import type { Agent } from "../../protocol";
import { accountConfigDir } from "../../accounts/accountDir";
import { loginStateOf } from "../../accounts/quotaFormat";
import type { QuotaReading } from "../../quota/useAccountQuota";
import { useDaemonStore, useProviders } from "../../store";

/** What takes the Log in button's place in an account's Status cell where its provider has no login flow: a help tip
 * that says how to log in with the agent's own command (`SignInHint`), shown while the account reads as not logged in
 * and is not turned off, and nothing otherwise. */
export function AccountSignInHelp({ agent, account, name, quota, turnedOff }: { agent: Agent; account: string | null; name: string; quota: QuotaReading; turnedOff: boolean }): React.ReactElement | null {
  const t = useT();
  const providers = useProviders();
  const dir = useDaemonStore((s) => accountConfigDir(s.settings.accounts, s.agentAvailability, providers, agent, account));
  if (turnedOff || loginStateOf(quota) !== "out" || offersLoginFlow(providers, agent) || dir === "") return null;
  return <HelpTip label={t("settings.accounts.signInHint.label", { name: isolate(name) })}><SignInHint providers={providers} agent={agent} dir={dir} /></HelpTip>;
}
