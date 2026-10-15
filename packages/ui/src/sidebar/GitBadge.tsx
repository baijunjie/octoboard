import { AlertTriangle, ArrowDown, ArrowUp, GitBranch, GitCommitHorizontal, RefreshCw } from "lucide-react";
import React from "react";

import { FadeOverflow } from "../components/FadeOverflow";
import { IndicatorTooltip } from "../components/IndicatorTooltip";
import { gitBadgeAriaLabel } from "../gitStatusLabel";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { GitStatus } from "../protocol";

function ScrollingCountArrow({ direction }: { direction: "up" | "down" }): React.ReactElement {
  const up = direction === "up";
  const Icon = up ? ArrowUp : ArrowDown;
  return (
    <span aria-hidden="true" className="relative size-3 shrink-0 overflow-clip">
      <span className={`relative block size-full ${up ? "motion-safe:animate-push-scroll" : "motion-safe:animate-pull-scroll"}`}>
        <Icon className="size-3" />
        <Icon className={`absolute inset-x-0 size-3 ${up ? "top-full" : "-top-full"}`} />
      </span>
    </span>
  );
}

/**
 * A project's branch and how far it is from its upstream, shown between the name and the row's
 * trailing controls (or, in focus mode, in the header). Renders nothing while there is no status
 * yet or the project is not a git repository (`GitStatus.repository` false).
 *
 * The glyph differs by `activity` and against the idle glyph, since motion alone is never the only
 * cue for a state: idle is a branch glyph (a commit glyph instead for a detached `HEAD`, so that is
 * told apart without reading the accessible name), and every in-flight activity a spinning
 * `RefreshCw`. The ahead count's arrow scrolls upward during a push; the behind count's arrow
 * scrolls downward during a fast-forward. Both scroll inside a fixed count-arrow-sized clip.
 * A failed check replaces the idle glyph with a red `AlertTriangle`; the in-flight glyph wins
 * over it while the next operation runs. The ahead/behind counts and the error glyph are likewise
 * never colour alone.
 *
 * The whole badge is one `IndicatorTooltip` (hover only, so it adds no tab stop to its row) whose
 * text is the accessible-name sentence itself: one tooltip for the glyph, the branch and both
 * counts, rather than one per part, so the branch name and the error can still be read once the
 * badge is squeezed down to the glyph and a clipped line. The one state with no tooltip is the one
 * showing "No commits yet" as visible text, which a tooltip would only repeat.
 *
 * `decorative` drops the composed `role="img"` accessible name for a badge sitting inside a tree
 * row, whose own `role="button"` already has to carry it (`withGitBadge` folds it into the row's
 * `aria-label` there). The tooltip's trigger is then unnamed and its contents are `aria-hidden`, as
 * the whole badge always was in a row, so the branch and the counts are not read twice; the
 * focus-mode header badge is not inside such a row, so its trigger keeps the name and its contents
 * stay exposed. Hiding the contents rather than the trigger is what `IndicatorTooltip` allows
 * without a change of its own, and it costs nothing: the trigger, which stays outside the hidden
 * wrapper, is the only focusable element here and is `tabIndex={-1}`, and nothing inside the
 * wrapper is focusable.
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
  // A lone glyph with no branch and no error (a freshly-`git init`'d repository) would otherwise
  // read as nothing at all to a sighted user, unlike every other state here, so this is the one
  // case where the badge falls back to the catalog's own wording, shown as visible text. That text
  // is all the badge says, so this is also the one state with no tooltip repeating it.
  const noCommitsYet = !status.branch && status.activity === "idle" && !status.error;
  // The project name has its own floor; this badge keeps giving way while the activity glyph
  // remains fixed. No cap: use the room the row offers before fading the shared text line. The
  // tooltip's trigger is the badge's own box, so it is the trigger that carries these.
  const className = "flex min-w-0 items-center gap-1 overflow-hidden text-xs text-app-muted";
  const content = (
    <>
      {status.activity !== "idle" ? (
        <RefreshCw aria-hidden="true" className="size-3.5 shrink-0 motion-safe:animate-spin-slow" />
      ) : status.error ? (
        <AlertTriangle aria-hidden="true" className="size-3.5 shrink-0 text-app-danger" />
      ) : status.detached ? (
        <GitCommitHorizontal aria-hidden="true" className="size-3.5 shrink-0" />
      ) : (
        <GitBranch aria-hidden="true" className="size-3.5 shrink-0" />
      )}
      {(status.branch || noCommitsYet || status.ahead > 0 || status.behind > 0) && (
        <FadeOverflow as="span" dir={status.branch || status.ahead > 0 || status.behind > 0 ? "ltr" : undefined} className="min-w-0 flex-1">
          <span className="inline-flex items-center gap-1">
            {status.branch ? <span>{status.branch}</span> : noCommitsYet && <span dir="auto">{t("sidebar.git.noCommitsYet")}</span>}
            {status.ahead > 0 && (
              <span className="inline-flex shrink-0 items-center">
                {status.activity === "pushing" ? <ScrollingCountArrow direction="up" /> : <ArrowUp aria-hidden="true" className="size-3 shrink-0" />}
                {status.ahead.toLocaleString(language)}
              </span>
            )}
            {status.behind > 0 && (
              <span className="inline-flex shrink-0 items-center">
                {status.activity === "pulling" ? <ScrollingCountArrow direction="down" /> : <ArrowDown aria-hidden="true" className="size-3 shrink-0" />}
                {status.behind.toLocaleString(language)}
              </span>
            )}
          </span>
        </FadeOverflow>
      )}
    </>
  );
  if (noCommitsYet) {
    const a11y = decorative ? ({ "aria-hidden": true } as const) : ({ role: "img", "aria-label": label } as const);
    return (
      <span {...a11y} className={className}>
        {content}
      </span>
    );
  }
  return (
    <IndicatorTooltip tooltip={label} label={decorative ? undefined : label} className={className}>
      {/* `contents` keeps the glyph and the shared text line the trigger's own flex items. */}
      {decorative ? (
        <span aria-hidden="true" className="contents">
          {content}
        </span>
      ) : (
        content
      )}
    </IndicatorTooltip>
  );
}
