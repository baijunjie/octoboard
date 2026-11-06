import { ChevronRight } from "lucide-react";
import React, { useEffect, useRef } from "react";

import { DaemonErrorMessage } from "../components/alerts/DaemonErrorMessage";
import { ErrorAlert } from "../components/alerts/ErrorAlert";
import { errorDetails } from "../components/alerts/errorParts";
import { useHasPainted } from "../components/motion/useHasPainted";
import { useListMotion } from "../components/motion/useListMotion";
import { FadedScroller } from "../components/scroll/scrollFade";
import { CountBadge } from "../components/status/CountBadge";
import { FadeOverflow } from "../components/text/FadeOverflow";
import type { ErrorPresentation } from "../errors/errorPresentation";
import { lacksSummary } from "../errors/daemonMessage";
import { isUncataloguedFailure } from "../errors/uncataloguedFailure";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import type { ProcessJump } from "./processJump";
import { Button } from "../primitives";

/** The attribute that marks the panel's root, for finding what holds focus. */
export const PANEL_ATTRIBUTE = "data-rail-processes-panel";

/** One line of the panel: a project or a console session with running trees, and where its Processes panel is. */
export interface ProcessRow {
  key: string;
  title: string;
  /** What the title sits in: the console of the project or console session. */
  context?: string;
  count: number;
  target: ProcessJump;
}

/** The popover's contents: a failed refresh, then a row for each of `rows`. A failure the application words says why
 * ahead of the counts being out of date; one it has no words for leaves its own text to Details. */
export function ProcessCounts({ rows, error, focusFirst, onPress }: {
  rows: ProcessRow[];
  error: ErrorPresentation | undefined;
  focusFirst: boolean;
  onPress: (target: ProcessJump) => void;
}): React.ReactElement {
  const t = useT();
  const root = useRef<HTMLDivElement>(null);
  // A failure already there as the popover opens is part of what it opened with; one that arrives while it is open is
  // announced.
  const hasPainted = useHasPainted();
  const listRef = useListMotion<HTMLUListElement>({ fitHeight: true });
  useEffect(() => {
    if (focusFirst) root.current?.querySelector<HTMLElement>("ul button")?.focus();
  }, []);
  return (
    <FadedScroller ref={root} {...{ [PANEL_ATTRIBUTE]: "" }} className="flex max-h-[70vh] flex-col gap-1 p-2">
      {error !== undefined && (
        <ErrorAlert status="warning" title={t("processes.refreshFailedTitle")} details={errorDetails(error)} announce={hasPainted}>
          {typeof error !== "string" && !isUncataloguedFailure(error) && !lacksSummary(error.code)
            ? <><DaemonErrorMessage error={error} part="summary" />{`${t("common.sentenceSeparator")}${t("processes.global.refreshFailed")}`}</>
            : t("processes.global.refreshFailed")}
        </ErrorAlert>
      )}
      <ul ref={listRef} className="relative flex min-w-0 flex-col gap-1">
        {rows.map((row) => (
          <li key={row.key} data-list-motion={row.key} className="min-w-0">
            <Button variant="ghost" size="sm" className="h-auto min-h-0 w-full justify-start gap-2 rounded-lg px-2 py-1.5 font-normal"
              label={row.context === undefined
                ? t("processes.global.show", { name: isolate(row.title), count: row.count })
                : t("processes.global.showIn", { name: isolate(row.title), console: isolate(row.context), count: row.count })}
              onPress={() => onPress(row.target)}>
              <span className="flex min-w-0 flex-1 flex-col text-start">
                <FadeOverflow as="span" dir="auto" className="block font-semibold" titleWhenClipped={row.title}>{row.title}</FadeOverflow>
                {row.context !== undefined && <FadeOverflow as="span" dir="auto" className="block text-xs text-app-muted" titleWhenClipped={row.context}>{row.context}</FadeOverflow>}
              </span>
              <CountBadge count={row.count} color="success" />
              <ChevronRight aria-hidden="true" className="-me-0.5 size-3.5 shrink-0 text-app-muted rtl:-scale-x-100" />
            </Button>
          </li>
        ))}
      </ul>
    </FadedScroller>
  );
}
