/** When a report page was pushed, as its header shows it: the time alone for a page pushed today, the date and time
 * otherwise, in both cases to the minute. `now` is when the panel draws. */
export function formatPageTime(createdAt: string | number, language: string, now: Date = new Date()): string {
  const at = new Date(createdAt);
  const today = at.getFullYear() === now.getFullYear() && at.getMonth() === now.getMonth() && at.getDate() === now.getDate();
  return at.toLocaleString(
    language,
    today
      ? { hour: "numeric", minute: "2-digit" }
      : { year: "numeric", month: "numeric", day: "numeric", hour: "numeric", minute: "2-digit" },
  );
}
