import type { Session } from "../protocol";

/** A mention being typed in the composer: the `@` at `start`, the text after it up to the caret (`query`), and `end`,
 * where the word it stands in ends, which a picked mention replaces up to. */
export interface MentionToken {
  start: number;
  end: number;
  query: string;
}

/**
 * The mention being typed at `caret` in `text`: an `@` at the start of the text or after whitespace, followed up to the
 * caret by no whitespace and no other `@`; none otherwise. An email address (`a@b`) is not one, since its `@` follows a
 * letter. The token runs on past the caret to the end of its word.
 */
export function mentionAt(text: string, caret: number): MentionToken | undefined {
  const match = /(?:^|\s)@([^\s@]*)$/u.exec(text.slice(0, caret));
  if (!match) return undefined;
  const query = match[1];
  let end = caret;
  while (end < text.length && !/\s/u.test(text[end])) end++;
  return { start: caret - query.length - 1, end, query };
}

/** `text` with the mention `token` replaced by `inserted` and a space after it (none where whitespace follows already),
 * and where the caret goes: after that space. */
export function applyMention(text: string, token: MentionToken, inserted: string): { text: string; caret: number } {
  const after = text.slice(token.end);
  const spaced = /^\s/u.test(after) ? inserted : `${inserted} `;
  return { text: text.slice(0, token.start) + spaced + after, caret: token.start + spaced.length };
}

/** What a session picked with `@` inserts: its title as shown and its full id, which a person can read and an agent can
 * pass to Octoboard's session tools. */
export function sessionMention(title: string, id: string): string {
  return `@「${title}」(session ${id})`;
}

/** Whether `text` contains `query`, ignoring case; an empty query is in everything. */
export function matchesMention(text: string, query: string): boolean {
  return text.toLowerCase().includes(query.toLowerCase());
}

/** How near `session` is to `current` for the picker: 0 in the same project, 1 in the same console, 2 elsewhere. */
function nearness(session: Session, current: Session): number {
  if (current.project_id !== null && session.project_id === current.project_id) return 0;
  return session.console_id === current.console_id ? 1 : 2;
}

/**
 * The sessions `@` offers in `current`'s composer, in the picker's order: every unarchived session but `current` whose
 * title (`title`, as shown) contains `query`, those of `current`'s project first, then those of its console, then
 * those of other consoles, the most recently started first within each.
 */
export function mentionSessions(sessions: Iterable<Session>, current: Session, query: string, title: (session: Session) => string): Session[] {
  const offered: Session[] = [];
  for (const session of sessions) {
    if (session.id === current.id || session.status === "archived") continue;
    if (matchesMention(title(session), query)) offered.push(session);
  }
  return offered.sort((a, b) => nearness(a, current) - nearness(b, current) || b.started_at - a.started_at);
}

/** One file or directory `@` offers: its path relative to the project's root, `/`-separated. */
export interface MentionPath {
  path: string;
  directory: boolean;
}

/** The directory part of a typed path (`src/ui` of `src/ui/Bu`, empty with no `/`) and the name part after it. */
export function splitMentionPath(query: string): { directory: string; name: string } {
  const slash = query.lastIndexOf("/");
  return slash === -1 ? { directory: "", name: query } : { directory: query.slice(0, slash), name: query.slice(slash + 1) };
}

/** The last part of a path. */
export function baseName(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

/**
 * The files and directories `@` offers for `query`, at most `limit`: first the entries `listed` of the directory the
 * query names (the project's own without a `/`) whose name contains its name part, directories first; then, for a
 * query with no `/`, the files a project-wide name search `found`, and the directories above them whose name contains
 * the query, each once.
 */
export function mentionPaths(query: string, listed: readonly MentionPath[], found: readonly string[], limit: number): MentionPath[] {
  const { name } = splitMentionPath(query);
  const seen = new Set<string>();
  const result: MentionPath[] = [];
  const add = (entry: MentionPath) => {
    if (seen.has(entry.path) || result.length >= limit) return;
    seen.add(entry.path);
    result.push(entry);
  };
  const matching = listed.filter((entry) => matchesMention(baseName(entry.path), name));
  for (const entry of matching) if (entry.directory) add(entry);
  for (const entry of matching) if (!entry.directory) add(entry);
  if (query.includes("/") || name === "") return result;
  const directories: string[] = [];
  for (const path of found) {
    const parts = path.split("/");
    for (let depth = 1; depth < parts.length; depth++) {
      const directory = parts.slice(0, depth).join("/");
      if (matchesMention(parts[depth - 1], name) && !directories.includes(directory)) directories.push(directory);
    }
  }
  for (const directory of directories) add({ path: directory, directory: true });
  for (const path of found) add({ path, directory: false });
  return result;
}
