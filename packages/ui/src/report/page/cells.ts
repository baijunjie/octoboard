// How the cell value of a page (a key-values item, a stat's figure, a table's cell) reads on the screen: in the
// language of the UI, so a number is grouped and a boolean is a word.
import { useMemo } from "react";

import { useCurrentLanguage, useT } from "../../i18n/react";
import type { CellValue } from "../../protocol";

/** The text of a cell value in the UI's language. */
export function useCellText(): (value: CellValue) => string {
  const language = useCurrentLanguage();
  const t = useT();
  const number = useMemo(() => new Intl.NumberFormat(language, { maximumFractionDigits: 20 }), [language]);
  return (value) => {
    if (typeof value === "boolean") return t(value ? "report.yes" : "report.no");
    if (typeof value === "number") return number.format(value);
    return value;
  };
}
