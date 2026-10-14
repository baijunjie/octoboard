import { Label, ListBox, Select, Tabs } from "@heroui/react";
import { FileCheck, FolderTree, FunnelX, GitBranch, GitFork, List, ListFilter, TriangleAlert } from "lucide-react";
import React, { useRef } from "react";

import { CenteredEmptyPanel } from "../components/EmptyPanel";
import { ErrorAlert } from "../components/ErrorAlert";
import { CodeBlock } from "../components/InlineCode";
import { FadeOverflow } from "../components/FadeOverflow";
import { IconToggle } from "../components/IconToggle";
import { PathText } from "../components/PathText";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { useTabScrollLabels } from "../components/tabScrollLabels";
import { useFocusHandoff } from "../components/useFocusHandoff";
import type { Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import { abbreviateHome } from "../pathDisplay";
import type { WorktreeInfo } from "../protocol";
import { useDaemonStore } from "../store";
import { displayWirePath } from "../wirePath";
import { BranchComparisonView } from "./BranchComparison";
import type { GitView as GitViewKind } from "./browserState";
import { changeLayout } from "./changeLayout";
import { ChangeFilterField } from "./ChangeFilterField";
import { ChangeList } from "./ChangeList";
import type { ChangeViewState } from "./changeView";
import type { ChangeItem } from "./changes";
import { GitFailure, GitLoading } from "./gitStates";
import type { ChangeList as ChangeListState } from "./useChangeList";
import type { SourceState } from "./useProjectSource";

/** How a worktree is named: by its branch, or by the commit a detached `HEAD` is at. Not isolated
 * here: the name is the item's `textValue` and the children of a `dir="auto"` element, neither of
 * which wants it. The detached wording is prose that isolates its own commit. */
function worktreeName(t: Translate, worktree: WorktreeInfo): string {
  if (worktree.branch !== null) return displayWirePath(worktree.branch);
  return worktree.head === null ? t("git.worktree.unborn") : t("git.worktree.detached", { commit: isolate(worktree.head.slice(0, 7)) });
}

/** What the worktree view shows and does: the chosen worktree's change list. */
export interface WorktreeViewProps {
  list: ChangeListState;
  /** The worktree whose changes are shown, by id; none for the one holding the project's
   * directory. */
  worktree: string | undefined;
  onWorktreeChange: (worktree: string | undefined) => void;
  selectedChange: string | undefined;
  onOpen: (item: ChangeItem) => void;
  listRef: React.Ref<HTMLDivElement>;
}

/**
 * The project pane's Git mode, in two views: a worktree's uncommitted changes, from the worktree
 * the selector chooses, and the changes between two local branches (`BranchComparisonView`). The
 * worktree only chooses where the uncommitted changes are read from: it checks nothing out, moves
 * no session, leaves the Files mode on the project's own directory and plays no part in a branch
 * comparison. A worktree that has gone is said to be unavailable, never replaced by another one;
 * the user chooses another.
 */
export function GitView(props: React.ComponentProps<typeof GitContent>): React.ReactElement {
  const t = useT();
  const { source, view, worktree, compare } = props;
  // One region for every loading the mode shows, outside `GitContent`, which swaps its whole tree
  // as the source loads. The source's loading and the worktree list's say the same words, so the
  // handoff between them leaves the text as it is and it is said once, not again for the list.
  let loading: string | undefined;
  if (source.state === "loading") loading = t("git.loading");
  else if (source.state === "loaded" && source.source.git) {
    if (view === "worktree") {
      const git = source.source.git;
      if (worktree.list.state === "loading" && !worktreeGone(git.worktrees, git.worktree, worktree.worktree, worktree.list)) loading = t("git.loading");
    } else if (compare.comparison.state === "loading") loading = t("git.compare.loading");
  }
  return (
    <>
      <StatusAnnouncer text={loading} />
      <GitContent {...props} />
    </>
  );
}

/** Whether the worktree the view shows has gone: no longer listed, or reported unavailable by the
 * list read from it. */
function worktreeGone(worktrees: WorktreeInfo[], own: string, worktree: string | undefined, list: ChangeListState): boolean {
  const shown = worktrees.find((w) => w.id === (worktree ?? own));
  return !shown || (list.state === "error" && list.unavailable);
}

function GitContent({
  source,
  view,
  onViewChange,
  worktree,
  compare,
  changeView,
  selectedSessionId,
  onRetry,
}: {
  source: SourceState;
  view: GitViewKind;
  onViewChange: (view: GitViewKind) => void;
  worktree: WorktreeViewProps;
  compare: Omit<React.ComponentProps<typeof BranchComparisonView>, "onRetry" | "changeView" | "selectedSessionId">;
  /** How both views' change lists show their changes. */
  changeView: ChangeViewState;
  /** The session selected in the sidebar, whose branches the Compare view marks. */
  selectedSessionId: string | undefined;
  /**
   * Asks again for whatever failed. `from` is the control pressed, which goes away with the
   * failure.
   */
  onRetry: (from: Element | null) => void;
}): React.ReactElement {
  const t = useT();
  const tabScrollLabels = useTabScrollLabels();
  if (source.state === "loading") return <GitLoading label={t("git.loading")} />;
  if (source.state === "error") return <GitFailure title={t("git.error.source")} message={source.message} onRetry={onRetry} />;
  const git = source.source.git;
  if (!git) {
    return source.source.git_error !== null ? (
      <div className="flex min-w-0 flex-1 items-center justify-center p-4">
        <ErrorAlert title={t("git.unavailable")} className="w-full max-w-lg">
          <CodeBlock>{source.source.git_error}</CodeBlock>
        </ErrorAlert>
      </div>
    ) : (
      <CenteredEmptyPanel icon={GitFork} message={t("git.notARepository")} />
    );
  }
  return (
    <Tabs
      variant="secondary"
      selectedKey={view}
      onSelectionChange={(key) => onViewChange(key as GitViewKind)}
      className="flex min-h-0 flex-1 flex-col gap-0"
    >
      {/* The filter and layout toggles are members of the tab row: the tabs' container draws only
          its list, and wrapping it would switch its variant's styles off, so they join it through
          the container's `render`. The tabs are `h-10` because HeroUI's small icon-only button is
          36px below 48rem and 32px above, and the row's height is the tabs'. */}
      <Tabs.ListContainer
        className="flex shrink-0 items-center ps-3 pe-2"
        render={(props) => (
          <div {...props}>
            {tabScrollLabels(props.children)}
            <ChangeViewToggles changeView={changeView} />
          </div>
        )}
      >
        <Tabs.List aria-label={t("git.views")} className="w-auto">
          {/* HeroUI dims a hovered tab to 70%, which takes its muted label under WCAG AA's 4.5:1;
              the hover darkens it instead. */}
          <Tabs.Tab id="worktree" className="h-10 px-3 whitespace-nowrap hover:text-foreground hover:opacity-100">
            {t("git.view.worktree")}
            <Tabs.Indicator />
          </Tabs.Tab>
          <Tabs.Tab id="compare" className="h-10 px-3 whitespace-nowrap hover:text-foreground hover:opacity-100">
            {t("git.view.compare")}
            <Tabs.Indicator />
          </Tabs.Tab>
        </Tabs.List>
      </Tabs.ListContainer>
      <Tabs.Panel id="worktree" className="mt-0 flex min-h-0 flex-1 flex-col p-0">
        <WorktreeChanges worktrees={git.worktrees} own={git.worktree} {...worktree} changeView={changeView} onRetry={onRetry} />
      </Tabs.Panel>
      <Tabs.Panel id="compare" className="mt-0 flex min-h-0 flex-1 flex-col p-0">
        <BranchComparisonView {...compare} changeView={changeView} selectedSessionId={selectedSessionId} onRetry={onRetry} />
      </Tabs.Panel>
    </Tabs>
  );
}

function WorktreeChanges({
  worktrees,
  own,
  list,
  worktree,
  onWorktreeChange,
  selectedChange,
  onOpen,
  onRetry,
  listRef,
  changeView,
}: WorktreeViewProps & {
  worktrees: WorktreeInfo[];
  own: string;
  changeView: ChangeViewState;
  onRetry: (from: Element | null) => void;
}): React.ReactElement {
  const t = useT();
  // When what holds focus below the selector goes — the list replaced by its empty state, a failure
  // or the worktree found gone — focus goes to the selector rather than falling to `<body>`.
  const selectRef = useRef<HTMLDivElement>(null);
  const handoff = useFocusHandoff(() => selectRef.current?.querySelector<HTMLElement>("button")?.focus());
  const shownId = worktree ?? own;
  const shown = worktrees.find((w) => w.id === shownId);
  const gone = worktreeGone(worktrees, own, worktree, list);
  return (
    <>
      <WorktreeSelect
        ref={selectRef}
        worktrees={worktrees}
        own={own}
        value={shownId}
        missing={!shown}
        onChange={(id) => onWorktreeChange(id === own ? undefined : id)}
      />
      <ChangeFilterField changeView={changeView} />
      <div className="flex min-h-0 flex-1 flex-col" onFocus={handoff.onFocus} onBlur={handoff.onBlur}>
        {shown && !gone && !shown.scope_present && <p className="shrink-0 px-3 py-2 text-xs text-muted">{t("git.worktree.scopeMissing")}</p>}
        {gone ? (
          <CenteredEmptyPanel icon={TriangleAlert} message={t("git.worktree.gone")} description={t("git.worktree.gone.description")} />
        ) : list.state === "loading" ? (
          <GitLoading label={t("git.loading")} />
        ) : list.state === "error" ? (
          <GitFailure title={t("git.error.list")} message={list.message} onRetry={onRetry} />
        ) : list.items.length === 0 && list.complete ? (
          <CenteredEmptyPanel icon={FileCheck} message={t("git.empty")} />
        ) : (
          <>
            <ChangeList items={list.items} selected={selectedChange} label={t("git.list.label")} onOpen={onOpen} listRef={listRef} changeView={changeView} />
            {!list.complete && <p className="shrink-0 border-t border-separator px-3 py-2 text-xs text-muted">{t("git.partial")}</p>}
          </>
        )}
      </div>
    </>
  );
}

/** The two view toggles at the end of the row of view tabs: the one that holds the filter field
 * the views draw (`ChangeFilterField`), then Group by folder. Closing the filter drops the text with the field,
 * so no filter out of sight narrows a list; the field hands keyboard focus back to the toggle as
 * it goes, which is what `filterToggleRef` is for. */
function ChangeViewToggles({ changeView }: { changeView: ChangeViewState }): React.ReactElement {
  const t = useT();
  return (
    <div className="ms-auto flex shrink-0 items-center gap-1">
      <IconToggle
        ref={changeView.filterToggleRef}
        label={t(changeView.filterOpen ? "git.filter.hide" : "git.filter.show")}
        isSelected={changeView.filterOpen}
        expanded={changeView.filterOpen}
        onChange={changeView.onFilterOpenChange}
      >
        {changeView.filterOpen ? <FunnelX aria-hidden="true" className="size-4" /> : <ListFilter aria-hidden="true" className="size-4" />}
      </IconToggle>
      <IconToggle
        label={t(changeView.layout === "tree" ? "git.layout.flat" : "git.layout.tree")}
        isSelected={changeView.layout === "tree"}
        onChange={(selected) => changeLayout.set(selected ? "tree" : "flat")}
      >
        {/* Both glyphs hang their rows off the leading side, so they mirror under right-to-left,
            as the sidebar's fold control does. */}
        {changeView.layout === "tree" ? <FolderTree aria-hidden="true" className="size-4 rtl:-scale-x-100" /> : <List aria-hidden="true" className="size-4 rtl:-scale-x-100" />}
      </IconToggle>
    </div>
  );
}

/** The worktree whose changes are shown. A worktree that is no longer listed stays the value, named
 * as gone, until the user picks another. Its field is the secondary variant, which draws from the
 * pane's `--default` fills like the filter field below it (`.control-fills`). */
function WorktreeSelect({
  ref,
  worktrees,
  own,
  value,
  missing,
  onChange,
}: {
  worktrees: WorktreeInfo[];
  own: string;
  value: string;
  missing: boolean;
  onChange: (worktree: string) => void;
  ref: React.Ref<HTMLDivElement>;
}): React.ReactElement {
  const t = useT();
  const homeDir = useDaemonStore((s) => s.homeDir);
  const current = worktrees.find((w) => w.id === value);
  const currentName = current ? worktreeName(t, current) : missing ? t("git.worktree.goneName") : "";
  return (
    <div ref={ref} className="shrink-0 border-b border-separator px-3 py-2">
      <Select fullWidth variant="secondary" value={value} onChange={(key) => key !== null && onChange(String(key))}>
        <Label className="sr-only">{t("git.worktree.label")}</Label>
        <Select.Trigger>
          <Select.Value className="flex min-w-0 items-center gap-2">
            {() => (
              <>
                <GitBranch aria-hidden="true" className="size-4 shrink-0 text-muted" />
                <FadeOverflow as="span" dir="auto" className="min-w-0 flex-1" titleWhenClipped={currentName}>
                  {currentName}
                </FadeOverflow>
              </>
            )}
          </Select.Value>
          <Select.Indicator />
        </Select.Trigger>
        <Select.Popover>
          <ListBox>
            {worktrees.map((worktree) => {
              const name = worktreeName(t, worktree);
              const shownName = worktree.id === own ? t("git.worktree.own", { name: worktree.branch === null ? name : isolate(name) }) : name;
              const path = abbreviateHome(displayWirePath(worktree.root), homeDir);
              // `pe-7` restores the room HeroUI reserves for the check mark, which its Select styles
              // override with a narrower padding, so a long name would run under the mark.
              return (
                <ListBox.Item key={worktree.id} id={worktree.id} textValue={name} className="pe-7">
                  <span className="flex min-w-0 flex-1 flex-col">
                    <FadeOverflow as="span" dir="auto" titleWhenClipped={shownName}>
                      {shownName}
                    </FadeOverflow>
                    <PathText as="span" path={path} className="text-xs text-muted" />
                    {!worktree.scope_present && <span className="text-xs text-muted">{t("git.worktree.scopeMissing")}</span>}
                  </span>
                  <ListBox.ItemIndicator />
                </ListBox.Item>
              );
            })}
          </ListBox>
        </Select.Popover>
      </Select>
    </div>
  );
}
