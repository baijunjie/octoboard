import type { Translate } from "./i18n/catalog";
import { isolate } from "./i18n/isolate";
import { joinPhrases, joinValues } from "./i18n/joinPhrases";
import type { Language } from "./i18n/languages";
import type { ProcessTree } from "./protocol";
import { formatRelativeTime, isJustNow } from "./relativeTime";
import { formatFileSize } from "./viewer/format";

/** What a process tree is doing, as one line of independent facts in `language`: its memory, the
 * ports it listens on (each isolated, joined with the language's "and" so they cannot be read as
 * facts of the line), when its top process started and, for a tree of several, how many processes
 * it holds. `now` is the caller's clock reading (`useNow`), so the age keeps moving. */
export function processFacts(t: Translate, language: Language, tree: ProcessTree, now: number): string {
  return joinPhrases(language, [
    t("processes.memory", { size: formatFileSize(language, tree.memory_bytes) }),
    ...(tree.ports.length > 0 ? [t("processes.ports", { count: tree.ports.length, ports: joinValues(language, tree.ports.map((port) => isolate(String(port)))) })] : []),
    isJustNow(tree.started_at, now) ? t("processes.startedNow") : t("processes.started", { age: formatRelativeTime(language, tree.started_at, now) }),
    ...(tree.process_count > 1 ? [t("processes.count", { count: tree.process_count })] : []),
  ]);
}
