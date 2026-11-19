import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import { useDaemonStore } from "../store";

/** What a task starts a session in: `name` is its project's where it has one, else its console's, and `words` says it as
 * a sentence part, a console session of its console or a project session of its project, by their current names. A task
 * never outlives its console or project, but one can briefly be held before its record is. */
export function useTarget(consoleId: string, projectId: string | null): { name: string; words: string } {
  const t = useT();
  const consoleName = useDaemonStore((s) => s.consoles.get(consoleId)?.name) ?? "";
  const projectName = useDaemonStore((s) => (projectId ? s.projects.get(projectId)?.name : undefined)) ?? "";
  return {
    name: projectId ? projectName : consoleName,
    words: projectId
      ? t("scheduledTasks.form.target.project", { project: isolate(projectName) })
      : t("scheduledTasks.form.target.console", { console: isolate(consoleName) }),
  };
}
