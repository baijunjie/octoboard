import type { ScheduledTask } from "../protocol";

/** What a task is called in the list, the page's heading and the confirmations: its name, else the first line of its
 * prompt, which is never blank. */
export function taskName(task: ScheduledTask): string {
  const title = task.launch.title?.trim();
  if (title) return title;
  return task.prompt.trim().split(/\r?\n/, 1)[0]!.trim();
}
