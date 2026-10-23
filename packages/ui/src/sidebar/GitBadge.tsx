import { ArrowDown, ArrowUp, CircleAlert, GitBranch, GitCommitHorizontal, RefreshCw } from "lucide-react";
import React from "react";

import { FadeOverflow } from "../components/FadeOverflow";
import { IndicatorTooltip } from "../components/IndicatorTooltip";
import { gitBadgeContent } from "../gitStatusCopy";
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
 * A failed check replaces the idle glyph with a red `CircleAlert`, the circled `!` of every
 * danger mark; the in-flight glyph wins over it while the next operation runs. The ahead/behind
 * counts and the error glyph are likewise never colour alone.
 *
 * The whole badge is one `IndicatorTooltip` (hover only, so it adds no tab stop to its row) whose
 * rich facts mirror the accessible-name sentence: one tooltip for the glyph, the branch and both
 * counts, rather than one per part, so the branch name and the error can still be read once the
 * branch name is squeezed away. The one state with no tooltip is the one
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
 * Only the branch name gives way and runs as a marquee. The grid reserves room for the state
 * glyph and upstream counts, keeping them visible even when none of the branch name fits.
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
  // A zero-minimum branch track lets the badge shrink to its glyph and counts. Its automatic
  // minimum width reserves those fixed tracks even when the surrounding row is squeezed.
  const className = "grid grid-flow-col grid-cols-[auto_minmax(0,1fr)] auto-cols-max items-center gap-1 px-1 text-xs text-app-muted";
  const content = (
    <>
      {status.activity !== "idle" ? (
        <RefreshCw aria-hidden="true" className="size-3.5 shrink-0 motion-safe:animate-spin-slow" />
      ) : status.error ? (
        <CircleAlert aria-hidden="true" className="size-3.5 shrink-0 text-danger-soft-foreground" />
      ) : status.detached ? (
        <GitCommitHorizontal aria-hidden="true" className="size-3.5 shrink-0" />
      ) : (
        <GitBranch aria-hidden="true" className="size-3.5 shrink-0" />
      )}
      {(status.branch || noCommitsYet) && (
        <FadeOverflow as="span" dir={status.branch ? "ltr" : undefined} className="min-w-0">
          {status.branch || <span dir="auto">{t("sidebar.git.noCommitsYet")}</span>}
        </FadeOverflow>
      )}
      {(status.ahead > 0 || status.behind > 0) && (
        <span dir="ltr" className="col-start-3 flex shrink-0 items-center gap-1 whitespace-nowrap">
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
    <IndicatorTooltip tooltip={gitBadgeContent(language, t, status)} label={decorative ? undefined : label} className={className}>
      {/* `contents` keeps the glyph, branch and counts the trigger's own grid items. */}
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
