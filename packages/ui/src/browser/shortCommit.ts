/** A commit as the UI names it: the first seven characters of its id, as `git` abbreviates one. */
export function shortCommit(commit: string): string {
  return commit.slice(0, 7);
}
