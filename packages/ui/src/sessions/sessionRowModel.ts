import type { Translate } from "../i18n/catalog";
import type { Language } from "../i18n/languages";
import { providerModelDisplay } from "../agents/providers";
import type { Account, Launch, ProviderInfo, Session, Settings } from "../protocol";
import { sessionRowName } from "./sessionLabel";
import { sessionModelLabel } from "./sessionModel";
import type { ShownStatusKind } from "./sessionStatus";

/** What every session row and card draws its agent mark, model line and accessible name from. */
export interface SessionRowModel {
  accounts: Account[];
  defaultNames: Settings["default_account_names"];
  launch: Launch;
  /** The model and effort beside the agent's mark, which is also the last part of `name`. */
  modelLabel: string;
  /** The providers the daemon serves, which name an agent that has no presentation module. */
  providers: readonly ProviderInfo[];
  /** The row's accessible name (`sessionRowName`). `owners` are the sessions it is listed under or
   * bound to, in the order it names them; an absent one is skipped. */
  name: string;
}

/** The model of a row whose accounts and launch the caller already holds (the archive view, which is
 * handed them rather than reading the store). */
export function sessionRowModel(
  t: Translate,
  language: Language,
  session: Session,
  owners: readonly (Session | undefined)[],
  status: ShownStatusKind,
  { accounts, defaultNames, launch, providers }: Pick<SessionRowModel, "accounts" | "defaultNames" | "launch" | "providers">,
): SessionRowModel {
  const modelLabel = sessionModelLabel(t, providerModelDisplay(providers, session.agent), launch);
  return { accounts, defaultNames, launch, modelLabel, providers, name: sessionRowName(t, language, session, { accounts, defaultNames, owners, modelLabel, status, providers }) };
}
