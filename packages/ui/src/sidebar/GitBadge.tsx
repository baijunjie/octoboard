import { AlertTriangle, ArrowDown, ArrowDownToLine, ArrowUp, GitBranch, GitCommitHorizontal, RefreshCw } from "lucide-react";
import React from "react";

import { FadeOverflow } from "../components/FadeOverflow";
import { IndicatorTooltip } from "../components/IndicatorTooltip";
import { gitBadgeAriaLabel } from "../gitStatusLabel";
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
    <IndicatorTooltip tooltip={t("sidebar.git.error", { error })} className="flex shrink-0 items-center">
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
 * As the badge is squeezed, the branch name gives way first — it fades out and then goes entirely,
 * leaving the glyph (the error one when the check failed) and the counts, which carry the same
 * facts between them — and only below that do the counts clip against `overflow-hidden`, in DOM
 * order. The glyph comes first for exactly that reason: it is the most important signal here, so
 * it has to be the last thing clipped, not the first.
 */
export function GitBadge({ status, decorative }: { status: GitStatus | undefined; decorative?: boolean }): React.ReactElement | null {
  const t = useT();
  const language = useCurrentLanguage();
  if (!status || !status.repository) return null;
  const a11y = decorative ? ({ "aria-hidden": true } as const) : ({ role: "img", "aria-label": gitBadgeAriaLabel(language, t, status) } as const);
  // A lone glyph with no branch and no error (a freshly-`git init`'d repository) would otherwise
  // read as nothing at all to a sighted user, unlike every other state here, so this is the one
  // case where the badge falls back to the catalog's own wording rather than leaving it to the
  // accessible name alone.
  const noCommitsYet = !status.branch && status.activity === "idle" && !status.error;
  return (
    // `min-w-0` is what lets the badge shrink at all: the project name has its own floor
    // (`min-w-16` on the flex item around `RowLabel` in `Sidebar.tsx` / `FocusView.tsx`), so past
    // that point the badge is what keeps giving way, and the branch name inside it is the only
    // part that can — everything else here is `shrink-0`. Deliberately uncapped: a cap that binds
    // while the row still has room clips the counts on an ordinary-width sidebar, which is the
    // common case, to protect a name that is not under pressure there. `overflow-hidden` is the
    // backstop for the width below which even the `shrink-0` glyphs no longer fit, so the row
    // itself never overflows.
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
      {status.branch ? (
        // `ltr`: a branch name is a path-like technical identifier, not text in the UI's language.
        // `clip="start"`: the distinguishing part of a long branch name (`feature/JIRA-1234-...`)
        // is its end, so that is what has to stay in view when it does not fit. No floor: a floor
        // would hold the branch open past the point where the badge can fit it, and what shows then
        // is a fragment cut mid-name against a hard edge — worse than letting it fade away and
        // leaving the glyph and the counts, which carry the same facts.
        <FadeOverflow as="span" dir="ltr" clip="start" className="min-w-0 flex-1" titleWhenClipped={status.branch}>
          {status.branch}
        </FadeOverflow>
      ) : (
        noCommitsYet && (
          <FadeOverflow as="span" className="min-w-0 flex-1" titleWhenClipped={t("sidebar.git.noCommitsYet")}>
            {t("sidebar.git.noCommitsYet")}
          </FadeOverflow>
        )
      )}
      {status.ahead > 0 && (
        <span className="flex shrink-0 items-center">
          <ArrowUp aria-hidden="true" className="size-3 shrink-0" />
          {status.ahead.toLocaleString(language)}
        </span>
      )}
      {status.behind > 0 && (
        <span className="flex shrink-0 items-center">
          <ArrowDown aria-hidden="true" className="size-3 shrink-0" />
          {status.behind.toLocaleString(language)}
        </span>
      )}
    </span>
  );
}
