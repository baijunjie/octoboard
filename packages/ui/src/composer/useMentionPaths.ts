import { useEffect } from "react";

import { listLane } from "../browse/budget";
import { useLatestRequest } from "../components/hooks/useLatestRequest";
import { isDirectory, treeEntries } from "../project-browser/tree";
import type { BrowseEntry } from "../protocol";
import { useDaemon } from "../store";
import { mentionPaths, splitMentionPath, type MentionPath } from "./mentions";

/** How long the picker waits for the last keystroke before it asks the daemon. */
export const MENTION_DEBOUNCE_MS = 150;

/** The most files and directories the picker lists at once. */
export const MENTION_PATH_LIMIT = 30;

export type MentionPaths =
  | { state: "loading"; paths: readonly MentionPath[] }
  | { state: "loaded"; paths: readonly MentionPath[] }
  | { state: "error"; paths: readonly MentionPath[] };

const NONE: readonly MentionPath[] = [];

/**
 * The files and directories `@` offers in `project` for `query` (`mentionPaths`), asked of the daemon while `active`, a
 * short wait after the last keystroke: the listing of the directory the query names, and for a query with no `/` the
 * project-wide file-name search Find in files' file-name filter runs. A directory that cannot be listed (one the user
 * is still typing) lists nothing rather than failing; a failed search is a failure. What was found last stays while
 * the next answer is out. Only the latest query's answer is taken, and a reconnect asks again.
 */
export function useMentionPaths(project: string | undefined, query: string, active: boolean): MentionPaths {
  const { request } = useDaemon();
  const latest = useLatestRequest<MentionPaths>({
    scope: project,
    initial: { state: "loading", paths: NONE },
    project,
  });

  useEffect(() => {
    if (!active || project === undefined) return;
    // What was found for the query before is not what this one finds: it shows as still being looked for.
    if (latest.value.state !== "loading") latest.hold((held) => ({ state: "loading", paths: held.paths }));
    const timer = setTimeout(() => {
      const { directory, name } = splitMentionPath(query);
      const slot = `mention:${project}`;
      const searches = !query.includes("/") && name !== "";
      void latest.send({
        lane: listLane,
        slot,
        request: async (current) => {
          const listed = await request({ type: "list_project_dir", project, path: directory, slot }).then(
            (reply): readonly BrowseEntry[] => (reply.project === project ? treeEntries(reply.entries) : []),
            (): readonly BrowseEntry[] => [],
          );
          if (!searches || !current()) return { listed, found: [] as readonly string[], project };
          const reply = await request({ type: "search_project_files", project, query: name, slot });
          return { listed, found: reply.query === name ? reply.paths : [], project: reply.project };
        },
        pending: (held) => ({ state: "loading", paths: held.paths }),
        accept: (reply) =>
          reply.project === project
            ? () => ({
              state: "loaded",
              paths: mentionPaths(
                query,
                reply.listed.map((entry) => ({ path: directory === "" ? entry.name : `${directory}/${entry.name}`, directory: isDirectory(entry) })),
                reply.found,
                MENTION_PATH_LIMIT,
              ),
            })
            : undefined,
        failed: () => ({ state: "error", paths: NONE }),
      });
    }, MENTION_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [project, query, active, latest.reconnects]);

  return project === undefined ? { state: "loaded", paths: NONE } : latest.value;
}
