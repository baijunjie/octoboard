import { format, isMessageKey } from "./i18n/catalog";
import type { Language } from "./i18n/languages";
import type { MessageParams } from "./protocol";
import type { State } from "./store";

/** The params that are record ids, and what each is shown as: the record's current name. */
const RECORD_NAMES: Record<string, (state: RecordNames, id: string) => string | undefined> = {
  console: (state, id) => state.consoles.get(id)?.name,
  project: (state, id) => state.projects.get(id)?.name,
  session: (state, id) => state.sessions.get(id)?.title,
};

type RecordNames = Pick<State, "consoles" | "projects" | "sessions">;

/** What the daemon's `error` or `session_notice` says, in `language`: the catalog's text
 * for its `code` with its params filled in, a record id shown as that record's name (the raw id only
 * when the record is unknown). A code the catalog does not have — a daemon newer than this client —
 * shows the daemon's own English `fallback`. A `reason_code` param is worded from the catalog's
 * `daemon.trust_reason.<code>` and replaces `reason`, which stays the daemon's English account for
 * a reason code the catalog lacks. A numeric `count` param selects the message's plural form. */
export function daemonMessage(
  language: Language,
  code: string,
  params: MessageParams,
  fallback: string,
  records: RecordNames,
): string {
  const key = `daemon.${code}`;
  if (!isMessageKey(key)) return fallback;
  const shown: Record<string, string | number> = Object.fromEntries(
    Object.entries(params).map(([name, value]) => [name, RECORD_NAMES[name]?.(records, value) ?? value]),
  );
  // A count arrives as text; as a number it selects the singular or plural wording.
  if (/^\d+$/.test(params.count ?? "")) shown.count = Number(params.count);
  const reasonKey = `daemon.trust_reason.${params.reason_code}`;
  if (isMessageKey(reasonKey)) shown.reason = format(language, reasonKey, shown);
  return format(language, key, shown);
}
