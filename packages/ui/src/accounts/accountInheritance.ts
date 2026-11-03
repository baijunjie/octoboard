import type { Account, Agent, Console, Project, ProjectAccount } from "../protocol";

/** The account `id` names for `agent` — `null`, the default account, when it names none or one
 * that is no longer stored. */
export function storedAccount(id: string | null | undefined, agent: Agent, accounts: Account[]): string | null {
  return id && accounts.some((account) => account.id === id && account.agent === agent) ? id : null;
}

/** The account the console refers to for `agent`, `null` (the default account) when none. */
export function consoleAccount(owner: Console, agent: Agent, accounts: Account[]): string | null {
  return storedAccount(owner.accounts[agent], agent, accounts);
}

/** `reference`, a project's setting for `agent`'s account, as the daemon reads it: one to an account
 * that is no longer stored counts as inheriting. */
export function settledProjectAccount(reference: ProjectAccount, agent: Agent, accounts: Account[]): ProjectAccount {
  return reference.kind === "account" && !storedAccount(reference.id, agent, accounts) ? { kind: "inherit" } : reference;
}

/** What the project says about `agent`'s account. */
export function projectAccount(project: Project, agent: Agent, accounts: Account[]): ProjectAccount {
  return settledProjectAccount(project.accounts[agent], agent, accounts);
}

/** The account a project session of `agent` runs under when none is chosen for it, as the daemon
 * resolves it: the project's `own` setting, which may be the default account and then stops the
 * chain, else the console's (`consoleAccountId`, `null` for none), else `null`, the default
 * account. */
export function resolveProjectAccount(own: ProjectAccount, consoleAccountId: string | null): string | null {
  if (own.kind === "default") return null;
  return own.kind === "account" ? own.id : consoleAccountId;
}

/** The account a session of `agent` runs under when none is chosen, using the project settings
 * when opening a project session and the console settings otherwise. */
export function inheritedAccount(owner: Console, project: Project | undefined, agent: Agent, accounts: Account[]): string | null {
  const account = consoleAccount(owner, agent, accounts);
  return project ? resolveProjectAccount(projectAccount(project, agent, accounts), account) : account;
}
