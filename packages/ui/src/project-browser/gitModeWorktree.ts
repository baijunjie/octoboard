/** The worktree each project's Git mode shows the uncommitted changes of, by id, while the browser
 * is in the Git mode: none for the worktree holding the project's directory, or once the browser has
 * left the Git mode. It stays as the browser last set it after the browser goes, for the window's
 * lifetime: it is where a new shell terminal starts (`terminal/shell/shellTerminals.ts`), which reads
 * it when the terminal is made, never during a render. */
const gitModeWorktrees = new Map<string, string>();

export function setGitModeWorktree(project: string, worktree: string | undefined): void {
  if (worktree === undefined) gitModeWorktrees.delete(project);
  else gitModeWorktrees.set(project, worktree);
}

export function gitModeWorktree(project: string): string | undefined {
  return gitModeWorktrees.get(project);
}

/** Drops the worktree of every project not among `projects`. */
export function forgetGitModeWorktreesOtherThan(projects: ReadonlySet<string>): void {
  for (const project of [...gitModeWorktrees.keys()]) if (!projects.has(project)) gitModeWorktrees.delete(project);
}
