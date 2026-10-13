import { AlertTriangle, ArrowDown, ArrowDownToLine, ArrowUp, GitBranch, GitCommitHorizontal, RefreshCw } from "lucide-react";
import React from "react";

import { FadeOverflow } from "../components/FadeOverflow";
import { IndicatorTooltip } from "../components/IndicatorTooltip";
import { gitBadgeAriaLabel } from "../gitStatusLabel";
import { isolate } from "../i18n/isolate";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { GitStatus } from "../protocol";

/**
 * Carries a failed check's message as a tooltip on the glyph, which is hover-only: the badge is a
 * non-interactive indicator, so it is an `IndicatorTooltip` and adds no tab stop to its row; the
 * message already reaches assistive technology through the badge's accessible name.
 */
function ErrorTooltip({ error, children }: { error: string | null | undefined; children: React.ReactNode }): React.ReactElement {
  const t = useT();
  if (!error) return <>{children}</>;
  return (
    <IndicatorTooltip tooltip={t("sidebar.git.error", { error: isolate(error) })} className="flex shrink-0 items-center">
      {children}
    </IndicatorTooltip>
  );
}

/**
 * A project's branch and how far it is from its upstream, shown between the name and the row's
 * trailing controls (or, in focus mode, in the header). Renders nothing while there is no status
 * yet or the project is not a git repository (`GitStatus.repository` false).
 *
 * The glyph differs by `activity` and against the idle glyph, since motion alone is never the only
 * cue for a state: idle is a branch glyph (a commit glyph instead for a detached `HEAD`, so that is
 * told apart without reading the accessible name), `checking` a spinning `RefreshCw` (contacting
 * the remote), `syncing` a nudging `ArrowDownToLine` (fast-forwarding the branch, which is
 * specifically a pull). A failed check replaces the idle glyph with a red `AlertTriangle` whose
 * tooltip carries the message; the in-flight glyphs win over it while the next check runs. The
 * ahead/behind counts and the error glyph are likewise never colour alone.
 *
 * `decorative` drops the composed `role="img"` accessible name for a badge sitting inside a tree
 * row, whose own `role="button"` already has to carry it (`withGitBadge` folds it into the row's
 * `aria-label` there); the focus-mode header badge is not inside such a row, so it keeps its own.
 *
 * No `role="status"` here: `activity` changing on its own (a sweep starting or finishing) is
 * exactly the kind of unprompted update that would otherwise need one, but a live region on every
 * row would announce each project's `checking` → `idle` roundtrip every five minutes, for every
 * project, which is noise rather than news. Left unannounced on purpose.
 *
 * The branch and upstream counts share one line: as the badge is squeezed, they fade and run as
 * a marquee together from the branch's beginning to the last count. The activity glyph stays
 * outside that line so it remains visible even when none of the text fits.
 */
export function GitBadge({ status, decorative }: { status: GitStatus | undefined; decorative?: boolean }): React.ReactElement | null {
  const t = useT();
  const language = useCurrentLanguage();
  if (!status || !status.repository) return null;
  const label = gitBadgeAriaLabel(language, t, status);
  const a11y = decorative ? ({ "aria-hidden": true } as const) : ({ role: "img", "aria-label": label } as const);
  // A lone glyph with no branch and no error (a freshly-`git init`'d repository) would otherwise
  // read as nothing at all to a sighted user, unlike every other state here, so this is the one
  // case where the badge falls back to the catalog's own wording rather than leaving it to the
  // accessible name alone.
  const noCommitsYet = !status.branch && status.activity === "idle" && !status.error;
  return (
    // The project name has its own floor; this badge keeps giving way while the activity glyph
    // remains fixed. No cap: use the room the row offers before fading the shared text line.
    <span {...a11y} className="flex min-w-0 items-center gap-1 overflow-hidden text-xs text-muted">
      <ErrorTooltip error={status.error}>
        {status.activity === "checking" ? (
          <RefreshCw aria-hidden="true" className="size-3.5 shrink-0 motion-safe:animate-spin-slow" />
        ) : status.activity === "syncing" ? (
          <ArrowDownToLine aria-hidden="true" className="size-3.5 shrink-0 motion-safe:animate-sync-nudge" />
        ) : status.error ? (
          <AlertTriangle aria-hidden="true" className="size-3.5 shrink-0 text-danger" />
        ) : status.detached ? (
          <GitCommitHorizontal aria-hidden="true" className="size-3.5 shrink-0" />
        ) : (
          <GitBranch aria-hidden="true" className="size-3.5 shrink-0" />
        )}
      </ErrorTooltip>
      {(status.branch || noCommitsYet || status.ahead > 0 || status.behind > 0) && (
        <FadeOverflow
          as="span"
          dir={status.branch || status.ahead > 0 || status.behind > 0 ? "ltr" : undefined}
          className="min-w-0 flex-1"
          titleWhenClipped={label}
        >
          <span className="inline-flex items-center gap-1">
            {status.branch ? <span>{status.branch}</span> : noCommitsYet && <span dir="auto">{t("sidebar.git.noCommitsYet")}</span>}
            {status.ahead > 0 && (
              <span className="inline-flex shrink-0 items-center">
                <ArrowUp aria-hidden="true" className="size-3 shrink-0" />
                {status.ahead.toLocaleString(language)}
              </span>
            )}
            {status.behind > 0 && (
              <span className="inline-flex shrink-0 items-center">
                <ArrowDown aria-hidden="true" className="size-3 shrink-0" />
                {status.behind.toLocaleString(language)}
              </span>
            )}
          </span>
        </FadeOverflow>
      )}
    </span>
  );
}
