import React from "react";

import { useCurrentLanguage, useT } from "../i18n/react";
import type { LineStats } from "../protocol";

/** A change's counted additions and removals, with the signed values read left to right. */
export function ChangeLineCounts({ stats }: { stats: LineStats }): React.ReactElement {
  const language = useCurrentLanguage();
  const t = useT();
  const number = new Intl.NumberFormat(language);
  return (
    <span className="inline-flex shrink-0 items-center text-xs tabular-nums">
      <span className="sr-only">{t("git.row.lines", stats)}</span>
      <span aria-hidden="true" dir="ltr" className="inline-flex gap-1.5">
        {stats.added > 0 && <span className="text-(--tint-success-fg)">+{number.format(stats.added)}</span>}
        {stats.removed > 0 && <span className="text-(--tint-danger-fg)">-{number.format(stats.removed)}</span>}
      </span>
    </span>
  );
}
