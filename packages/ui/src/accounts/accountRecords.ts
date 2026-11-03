import type { AccountLogin, Agent } from "../protocol";

/** What names an account (`null` for an agent's default account) among records of several agents' accounts: a choice
 * of account, a login. */
export function accountKey(agent: Agent, account: string | null): string {
  return `${agent}:${account ?? ""}`;
}

/** The record of an account (`null` for an agent's default account) among `records`, if there is one. */
export function accountRecordOf<T extends { agent: Agent; account: string | null }>(records: readonly T[], agent: Agent, account: string | null): T | undefined {
  return records.find((record) => record.agent === agent && record.account === account);
}

/** The login of an account (`null` for an agent's default account), if one is running or has just ended. */
export function accountLoginOf(logins: readonly AccountLogin[], agent: Agent, account: string | null): AccountLogin | undefined {
  return accountRecordOf(logins, agent, account);
}

/** Whether the login has not ended: the CLI is running and the user may cancel it. */
export function loginIsActive(login: AccountLogin | undefined): boolean {
  return login !== undefined && (login.status === "starting" || login.status === "waiting");
}
