import type { BranchInfo } from "../protocol";

/** The branch `session` is responsible for: of those whose origin names it as responsible, the one
 * it became responsible for last. */
export function sessionBranch(branches: readonly BranchInfo[], session: string): string | undefined {
  let latest: { name: string; since: number } | undefined;
  for (const { name, origin } of branches) {
    if (origin?.responsible_session !== session) continue;
    if (!latest || origin.responsible_since > latest.since) latest = { name, since: origin.responsible_since };
  }
  return latest?.name;
}
