import { Button, Label, ListBox, Select, Spinner } from "@heroui/react";
import { FileCheck, GitBranch, GitFork, TriangleAlert } from "lucide-react";
import React, { useRef } from "react";

import { EmptyPanel } from "../components/EmptyPanel";
import { PathText } from "../components/PathText";
import { useFocusHandoff } from "../components/useFocusHandoff";
import type { Translate } from "../i18n/catalog";
import { useT } from "../i18n/react";
import { abbreviateHome } from "../pathDisplay";
import type { WorktreeInfo } from "../protocol";
import { useDaemonStore } from "../store";
import { displayWirePath } from "../wirePath";
import { ChangeList } from "./ChangeList";
import type { ChangeItem } from "./changes";
import type { ChangeList as ChangeListState } from "./useChangeList";
import type { SourceState } from "./useProjectSource";

/** How a worktree is named: by its branch, or by the commit a detached `HEAD` is at. */
function worktreeName(t: Translate, worktree: WorktreeInfo): string {
  if (worktree.branch !== null) return displayWirePath(worktree.branch);
  return worktree.head === null ? t("git.worktree.unborn") : t("git.worktree.detached", { commit: worktree.head.slice(0, 7) });
}

/**
 * The project pane's Git mode: which worktree's uncommitted changes are shown, and the changes. The
 * worktree only chooses where the changes are read from: it checks nothing out, moves no session
 * and leaves the Files mode on the project's own directory. A worktree that has gone is said to be
 * unavailable, never replaced by another one; the user chooses another.
 */
export function GitView({
  source,
  list,
  worktree,
  onWorktreeChange,
  selectedChange,
  onOpen,
  onRetry,
  listRef,
}: {
  source: SourceState;
  list: ChangeListState;
  /**
   * The worktree whose changes are shown, by id; none for the one holding the project's directory.
   */
  worktree: string | undefined;
  onWorktreeChange: (worktree: string | undefined) => void;
  selectedChange: string | undefined;
  onOpen: (item: ChangeItem) => void;
  /**
   * Asks again for whatever failed. `from` is the control pressed, which goes away with the
   * failure.
   */
  onRetry: (from: Element | null) => void;
  listRef: React.Ref<HTMLDivElement>;
}): React.ReactElement {
  const t = useT();
  // When what holds focus below the selector goes — the list replaced by its empty state, a failure
  // or the worktree found gone — focus goes to the selector rather than falling to `<body>`.
  const selectRef = useRef<HTMLDivElement>(null);
  const handoff = useFocusHandoff(() => selectRef.current?.querySelector<HTMLElement>("button")?.focus());
  if (source.state === "loading") return <Loading label={t("git.loading")} />;
  if (source.state === "error") return <Failure title={t("git.error.source")} message={source.message} onRetry={onRetry} />;
  const git = source.source.git;
  if (!git) {
    return source.source.git_error !== null ? (
      <EmptyPanel icon={TriangleAlert} message={t("git.unavailable", { reason: source.source.git_error })} />
    ) : (
      <EmptyPanel icon={GitFork} message={t("git.notARepository")} />
    );
  }
  const own = git.worktree;
  const shownId = worktree ?? own;
  const shown = git.worktrees.find((w) => w.id === shownId);
  const gone = !shown || (list.state === "error" && list.unavailable);
  return (
    <>
      <WorktreeSelect
        ref={selectRef}
        worktrees={git.worktrees}
        own={own}
        value={shownId}
        missing={!shown}
        onChange={(id) => onWorktreeChange(id === own ? undefined : id)}
      />
      <div className="flex min-h-0 flex-1 flex-col" onFocus={handoff.onFocus} onBlur={handoff.onBlur}>
        {!gone && !shown.scope_present && <p className="shrink-0 px-3 py-2 text-xs text-muted">{t("git.worktree.scopeMissing")}</p>}
        {gone ? (
          <EmptyPanel icon={TriangleAlert} message={t("git.worktree.gone")} />
        ) : list.state === "loading" ? (
          <Loading label={t("git.loading")} />
        ) : list.state === "error" ? (
          <Failure title={t("git.error.list")} message={list.message} onRetry={onRetry} />
        ) : list.items.length === 0 && list.complete ? (
          <EmptyPanel icon={FileCheck} message={t("git.empty")} />
        ) : (
          <>
            <ChangeList items={list.items} selected={selectedChange} label={t("git.list.label")} onOpen={onOpen} listRef={listRef} />
            {!list.complete && <p className="shrink-0 border-t border-separator px-3 py-2 text-xs text-muted">{t("git.partial")}</p>}
          </>
        )}
      </div>
    </>
  );
}

/** The worktree whose changes are shown. A worktree that is no longer listed stays the value, named
 * as gone, until the user picks another. */
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
  return (
    <div ref={ref} className="shrink-0 border-b border-separator px-3 py-2">
      <Select fullWidth value={value} onChange={(key) => key !== null && onChange(String(key))}>
        <Label className="sr-only">{t("git.worktree.label")}</Label>
        <Select.Trigger>
          <Select.Value className="flex min-w-0 items-center gap-2">
            {() => (
              <>
                <GitBranch aria-hidden="true" className="size-4 shrink-0 text-muted" />
                <span dir="auto" className="min-w-0 truncate">
                  {current ? worktreeName(t, current) : missing ? t("git.worktree.goneName") : ""}
                </span>
              </>
            )}
          </Select.Value>
          <Select.Indicator />
        </Select.Trigger>
        <Select.Popover>
          <ListBox>
            {worktrees.map((worktree) => {
              const name = worktreeName(t, worktree);
              const path = abbreviateHome(displayWirePath(worktree.root), homeDir);
              return (
                <ListBox.Item key={worktree.id} id={worktree.id} textValue={name}>
                  <span className="flex min-w-0 flex-col">
                    <span dir="auto" className="truncate">
                      {worktree.id === own ? t("git.worktree.own", { name }) : name}
                    </span>
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

function Loading({ label }: { label: string }): React.ReactElement {
  return (
    <div role="status" className="flex flex-1 items-center justify-center gap-2 text-sm text-muted">
      <Spinner size="sm" aria-hidden="true" />
      {label}
    </div>
  );
}

/** A source or a list that could not be read: why, and a way to try again. */
function Failure({
  title,
  message,
  onRetry,
}: {
  title: string;
  message: string;
  onRetry: (from: Element | null) => void;
}): React.ReactElement {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null);
  return (
    <div ref={ref} className="flex flex-1 flex-col items-center justify-center gap-3 px-4 text-center text-sm">
      <TriangleAlert aria-hidden="true" className="size-6 text-danger" />
      <div role="alert" className="flex flex-col gap-1">
        <p className="font-medium">{title}</p>
        <p className="text-muted">{message}</p>
      </div>
      <Button size="sm" variant="secondary" preventFocusOnPress onPress={() => onRetry(ref.current)}>
        {t("error.tryAgain")}
      </Button>
    </div>
  );
}
