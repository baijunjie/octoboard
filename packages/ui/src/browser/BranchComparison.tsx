import { Button, Label, ListBox, Select } from "@heroui/react";
import { ArrowUpDown, GitBranch, GitCompareArrows, GitFork, TriangleAlert } from "lucide-react";
import type React from "react";
import { ListLayout, Virtualizer } from "react-aria-components";

import { CenteredEmptyPanel } from "../components/EmptyPanel";
import { FadeOverflow } from "../components/FadeOverflow";
import { TitledControl } from "../components/TitledControl";
import { ROW_ICON_BUTTON_CLASS } from "../components/RowControls";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { useScrollFade } from "../components/scrollFade";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { BranchInfo } from "../protocol";
import { displayWirePath } from "../wirePath";
import { BranchOriginBadge } from "./BranchOriginBadge";
import { ChangeFilterField } from "./ChangeFilterField";
import { ChangeList } from "./ChangeList";
import type { ChangeViewState } from "./changeView";
import type { ChangeItem } from "./changes";
import { comparisonNotices } from "./comparisonNotices";
import { GitFailure, GitLoading } from "./gitStates";
import type { BranchList } from "./useBranchList";
import type { Comparison } from "./useBranchComparison";
import { shortCommit } from "./useChangeReader";

/** Every option in a branch selector is one line of this height, which is what lets a repository's
 * thousands of branches be listed virtualized. */
const BRANCH_ROW_HEIGHT = 32;

/**
 * The Git mode's comparison of two local branches: a selector for each — From, the base, and To —
 * and the changes To has made to the project since it forked from From, from their merge base to
 * To's tip (as `git diff From...To`). A comparison is of the commits the branches were at when it
 * was made, which it names, and the fork they share; when the
 * branch list read since says one of them has moved or gone, it says so, and Refresh compares the
 * branches as they are now. Nothing here checks anything out, and the worktree chosen for the
 * uncommitted changes plays no part in it.
 */
export function BranchComparisonView({
  branches,
  comparison,
  left,
  right,
  onBranchesChange,
  selectedChange,
  onOpen,
  onRetry,
  listRef,
  selectorsRef,
  changeView,
  selectedSessionId,
}: {
  branches: BranchList;
  comparison: Comparison;
  left: string | undefined;
  right: string | undefined;
  onBranchesChange: (left: string | undefined, right: string | undefined) => void;
  selectedChange: string | undefined;
  onOpen: (item: ChangeItem) => void;
  onRetry: (from: Element | null) => void;
  listRef: React.Ref<HTMLDivElement>;
  /** The selectors' strip, which takes keyboard focus when what held it below goes. */
  selectorsRef: React.RefObject<HTMLDivElement | null>;
  changeView: ChangeViewState;
  /** The session selected in the sidebar, whose branches the creator badge marks. */
  selectedSessionId: string | undefined;
}): React.ReactElement {
  const t = useT();
  // When what holds focus below the selectors goes — the list replaced by a failure or an empty
  // state — focus goes to the first selector that can take it rather than falling to `<body>`;
  // with nothing chosen in a repository that has no branches every control in the strip is
  // disabled, and focus leaves for the top bar as it does from any pane with nothing to hold it.
  const handoff = useFocusHandoff(() => selectorsRef.current?.querySelector<HTMLElement>("button:not([disabled])")?.focus());
  const listed = branches.state === "loaded" ? branches.branches : [];
  // Nothing chosen in a repository that has no branches: the selectors have nothing to offer, and
  // the panel below says so. Keyed on the same state as that panel, so a comparison already made
  // keeps its selectors even if the branches it names go.
  const noBranches = comparison.state === "idle" && branches.state === "loaded" && listed.length === 0;
  const known = (name: string | undefined) => name === undefined || branches.state !== "loaded" || !branches.complete || listed.some((b) => b.name === name);
  // A list that failed to refresh stays, and says why; one that never came says so in its place.
  const branchesError = branches.state === "error" ? branches.message : branches.state === "loaded" ? branches.error : undefined;
  return (
    <>
      <div ref={selectorsRef} className="flex shrink-0 items-center gap-1 border-b border-separator py-2 ps-3 pe-2">
        {/* One grid for both selectors, each a subgrid of it, so the label column is as wide as
            the widest label in the current language. */}
        <div className="grid min-w-0 flex-1 grid-cols-[auto_minmax(0,1fr)] items-center gap-x-3 gap-y-2">
          <BranchSelect
            label={t("git.compare.from")}
            branches={listed}
            value={left}
            gone={!known(left)}
            isDisabled={noBranches}
            selectedSessionId={selectedSessionId}
            onChange={(name) => onBranchesChange(name, right)}
          />
          <BranchSelect
            label={t("git.compare.to")}
            branches={listed}
            value={right}
            gone={!known(right)}
            isDisabled={noBranches}
            selectedSessionId={selectedSessionId}
            onChange={(name) => onBranchesChange(left, name)}
          />
        </div>
        <TitledControl title={t("git.compare.swap")}>
          <Button
            isIconOnly
            size="sm"
            variant="ghost"
            className={ROW_ICON_BUTTON_CLASS}
            aria-label={t("git.compare.swap")}
            isDisabled={left === undefined && right === undefined}
            preventFocusOnPress
            onPress={() => onBranchesChange(right, left)}
          >
            <ArrowUpDown aria-hidden="true" className="size-4" />
          </Button>
        </TitledControl>
      </div>
      <ChangeFilterField changeView={changeView} />
      {branchesError !== undefined && !(comparison.state === "idle" && branches.state === "error") && (
        <p role="alert" className="shrink-0 px-3 py-2 text-xs text-app-muted">
          {t("git.compare.branchesError", { message: isolate(branchesError) })}
        </p>
      )}
      <div className="flex min-h-0 flex-1 flex-col" onFocus={handoff.onFocus} onBlur={handoff.onBlur}>
        <ComparisonContent
          noBranches={noBranches}
          branches={branches}
          comparison={comparison}
          selectedChange={selectedChange}
          onOpen={onOpen}
          onRetry={onRetry}
          listRef={listRef}
          changeView={changeView}
        />
      </div>
    </>
  );
}

function ComparisonContent({
  noBranches,
  branches,
  comparison,
  selectedChange,
  onOpen,
  onRetry,
  listRef,
  changeView,
}: {
  /** Set where the selectors are disabled, so the panel explaining them cannot drift from them. */
  noBranches: boolean;
  branches: BranchList;
  comparison: Comparison;
  selectedChange: string | undefined;
  onOpen: (item: ChangeItem) => void;
  onRetry: (from: Element | null) => void;
  listRef: React.Ref<HTMLDivElement>;
  changeView: ChangeViewState;
}): React.ReactElement {
  const t = useT();
  if (comparison.state === "idle") {
    if (branches.state === "error") return <GitFailure title={t("git.compare.branchesFailed")} message={branches.message} onRetry={onRetry} />;
    if (noBranches) return <CenteredEmptyPanel icon={GitFork} message={t("git.compare.noBranches")} />;
    return <CenteredEmptyPanel icon={GitCompareArrows} message={t("git.compare.prompt")} description={t("git.compare.prompt.description")} />;
  }
  if (comparison.state === "loading") return <GitLoading label={t("git.compare.loading")} />;
  if (comparison.state === "error") return <GitFailure title={t("git.compare.error")} message={comparison.message} onRetry={onRetry} />;
  const { left, right, base, items, complete } = comparison;
  const notices = comparisonNotices(branches, comparison);
  return (
    <>
      <p className="shrink-0 px-3 py-2 text-xs text-app-muted">
        {t("git.compare.commits", { base: isolate(shortCommit(base)), to: isolate(shortCommit(right.commit)) })}
      </p>
      {/* Mounted for as long as the comparison is, so a notice appearing in it is announced. */}
      <div role="status" className="flex shrink-0 flex-col gap-1 px-3 text-xs text-app-muted not-empty:pb-2">
        {notices.map((notice) => (
          <p key={notice.branch} className="flex items-start gap-1.5">
            <TriangleAlert aria-hidden="true" className="mt-px size-3.5 shrink-0" />
            <span>
              {notice.kind === "moved" ? (
                <Message id="git.compare.moved" params={{ branch: <bdi dir="auto">{displayWirePath(notice.branch)}</bdi> }} />
              ) : (
                <Message id="git.compare.deleted" params={{ branch: <bdi dir="auto">{displayWirePath(notice.branch)}</bdi> }} />
              )}
            </span>
          </p>
        ))}
      </div>
      {left.commit === right.commit ? (
        <CenteredEmptyPanel icon={GitCompareArrows} message={t("git.compare.noDifferences")} description={t("git.compare.same.description")} />
      ) : items.length === 0 && complete ? (
        <CenteredEmptyPanel icon={GitCompareArrows} message={t("git.compare.noDifferences")} description={t("git.compare.empty.description")} />
      ) : (
        <>
          <ChangeList items={items} selected={selectedChange} label={t("git.compare.list.label")} onOpen={onOpen} listRef={listRef} changeView={changeView} />
          {!complete && <p className="shrink-0 border-t border-separator px-3 py-2 text-xs text-app-muted">{t("git.partial")}</p>}
        </>
      )}
    </>
  );
}

/** One end of the comparison: its label in the grid's first column, its trigger in the second. A
 * branch the list no longer has stays the value, named as gone, until the user picks another. */
export function BranchSelect({
  label,
  branches,
  value,
  gone,
  isDisabled,
  selectedSessionId,
  onChange,
}: {
  label: string;
  branches: BranchInfo[];
  value: string | undefined;
  gone: boolean;
  /** For a repository that has no branches to offer: a selector that could only open on an empty
   * list is a control with nothing behind it. */
  isDisabled?: boolean;
  selectedSessionId: string | undefined;
  onChange: (branch: string) => void;
}): React.ReactElement {
  const t = useT();
  const branchFade = useScrollFade({ className: "max-h-80 overflow-y-auto px-0" });
  const name = value === undefined ? undefined : displayWirePath(value);
  const shown =
    name === undefined ? t("git.compare.choose") : gone ? <Message id="git.compare.branchGone" params={{ branch: <bdi dir="auto">{name}</bdi> }} /> : name;
  const title = name === undefined ? undefined : gone ? t("git.compare.branchGone", { branch: isolate(name) }) : name;
  return (
    // The subgrid restates the column gap: HeroUI's `.select` sets a gap of its own, which would
    // otherwise replace the parent grid's for this row's gutter. The secondary variant draws from
    // the pane's `--default` fills (`.control-fills`), like the worktree selector and the filter field.
    <Select
      fullWidth
      variant="secondary"
      isDisabled={isDisabled}
      value={value ?? null}
      onChange={(key) => key !== null && onChange(String(key))}
      className="col-span-2 grid grid-cols-subgrid items-center gap-x-3"
    >
      <Label className="text-sm">{label}</Label>
      <Select.Trigger className="group min-w-0">
        <Select.Value className="flex min-w-0 items-center gap-2">
          {() => (
            <>
              <GitBranch aria-hidden="true" className="size-4 shrink-0 text-app-muted" />
              <FadeOverflow
                as="span"
                dir="auto"
                className="min-w-0 flex-1"
                titleWhenClipped={title}
              >
                {shown}
              </FadeOverflow>
            </>
          )}
        </Select.Value>
        <Select.Indicator />
      </Select.Trigger>
      {/* As wide as the trigger: the virtualized list lays its rows out to the popover's width
          rather than taking one from them, and without a width of its own the popover grows past
          the window. */}
      <Select.Popover className="w-(--trigger-width)">
        <Virtualizer layout={ListLayout} layoutOptions={{ rowHeight: BRANCH_ROW_HEIGHT }}>
          {/* The side inset moves from the scroll container onto the rows: the virtualized layout
              makes each row as wide as the list's inner width and places it inside the list's own
              padding, so a padded list overflows sideways and clips the rows' focus ring. `pe-7` on
              a row restores the room HeroUI reserves for the check mark, which its Select styles
              override with a narrower padding, so the commit would run under the mark. */}
          {/* The fade goes on the list, never on the popover around it: the popover is what animates
              away, and the fade's scroll-timeline animation never ends, so the popover's exit
              transition would wait for it forever. */}
          <ListBox items={branches} {...branchFade}>
            {(branch) => {
              const branchName = displayWirePath(branch.name);
              return (
                <ListBox.Item id={branch.name} textValue={branchName} className="mx-1.5 h-8 min-h-0 w-auto py-0 pe-7">
                  <FadeOverflow as="span" dir="auto" className="min-w-0 flex-1" titleWhenClipped={branchName}>
                    {branchName}
                  </FadeOverflow>
                  {branch.origin && <BranchOriginBadge origin={branch.origin} selectedSessionId={selectedSessionId} />}
                  <span dir="ltr" className="shrink-0 font-mono text-xs text-app-muted">
                    {shortCommit(branch.commit)}
                  </span>
                  <ListBox.ItemIndicator />
                </ListBox.Item>
              );
            }}
          </ListBox>
        </Virtualizer>
      </Select.Popover>
    </Select>
  );
}
