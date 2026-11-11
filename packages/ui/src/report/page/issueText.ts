import type { Translate } from "../../i18n/catalog";
import { isolate } from "../../i18n/isolate";
import type { FieldIssue } from "./formModel";

/** A limit a date, time or date and time field gave, as its control's value (`YYYY-MM-DD`, `HH:MM` or
 * `YYYY-MM-DDTHH:MM`), written the way `language` writes it. */
function momentText(language: string, limit: string): string {
  const [date, time] = limit.includes("T") ? limit.split("T") : /^\d{1,2}:\d{2}/.test(limit) ? [undefined, limit] : [limit, undefined];
  const [year = 1970, month = 1, day = 1] = (date ?? "").split("-").map(Number);
  const [hour = 0, minute = 0] = (time ?? "").split(":").map(Number);
  const moment = new Date(year, month - 1, day, hour, minute);
  if (Number.isNaN(moment.getTime())) return limit;
  return new Intl.DateTimeFormat(language, {
    ...(date === undefined ? {} : { dateStyle: "medium" as const }),
    ...(time === undefined ? {} : { timeStyle: "short" as const }),
  }).format(moment);
}

/** What is wrong with a field, in the language of the window. The values the page gave (a limit, a step) are written
 * the way `language` writes numbers, dates and times, and isolated, as a number or a date inside a right-to-left
 * sentence would otherwise be reordered against it. */
export function issueText(t: Translate, language: string, issue: FieldIssue): string {
  const number = (value: number) => isolate(new Intl.NumberFormat(language, { maximumFractionDigits: 20 }).format(value));
  switch (issue.kind) {
    case "required":
      return t("report.invalid.required");
    case "requiredChoice":
      return t("report.invalid.requiredChoice");
    case "requiredSelection":
      return t("report.invalid.requiredSelection");
    case "pattern":
      return t("report.invalid.pattern");
    case "patternUnchecked":
      return t("report.invalid.patternUnchecked");
    case "number":
      return t("report.invalid.number");
    case "minLength":
      return t("report.invalid.minLength", { count: issue.count });
    case "maxLength":
      return t("report.invalid.maxLength", { count: issue.count });
    case "minSelected":
      return t("report.invalid.minSelected", { count: issue.count });
    case "maxSelected":
      return t("report.invalid.maxSelected", { count: issue.count });
    case "min":
      return t("report.invalid.min", { min: number(issue.limit) });
    case "max":
      return t("report.invalid.max", { max: number(issue.limit) });
    case "notBefore":
      return t("report.invalid.notBefore", { min: isolate(momentText(language, issue.limit)) });
    case "notAfter":
      return t("report.invalid.notAfter", { max: isolate(momentText(language, issue.limit)) });
    case "step":
      return t("report.invalid.step", { step: number(issue.step) });
  }
}
