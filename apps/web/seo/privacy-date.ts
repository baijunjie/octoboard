// A policy revision date, not the time at which a page is built or opened.
export const privacyUpdatedDate = "2026-11-13";

export function formatPrivacyDate(language: string): string {
  return new Intl.DateTimeFormat(language, {
    year: "numeric",
    month: "long",
    day: "numeric",
    timeZone: "UTC",
  }).format(new Date(`${privacyUpdatedDate}T00:00:00Z`));
}
