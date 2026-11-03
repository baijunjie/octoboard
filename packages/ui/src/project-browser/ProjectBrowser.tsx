import { Tabs } from "@heroui/react";
import { RefreshCw } from "lucide-react";
import React, { useEffect, useId, useRef, useState } from "react";

import { Button } from "../components/controls/Button";
import { StatusMark } from "../components/status/StatusMark";
import { handFocusOff } from "../components/focus/handFocusOff";
import { HandOffOnUnmount } from "../components/focus/HandOffOnUnmount";
import { handFocusToRegionStart } from "../components/focus/modalFocus";
import { LocalizedTabListContainer } from "../components/tabs/LocalizedTabListContainer";
import { TEXT_TAB_BASE_CLASS, TEXT_TAB_LIST_CLASS } from "../components/tabs/textTabClasses";
import { TitledControl } from "../components/hint/TitledControl";
import { ROW_ICON_BUTTON_CLASS } from "../components/controls/RowControls";
import { useFocusHandoff } from "../components/focus/useFocusHandoff";
import { useT } from "../i18n/react";
import { PanelHeader } from "../components/panel/PanelHeader";
import type { Project } from "../protocol";
import { useDaemonStore } from "../store";
import { FileViewer } from "../viewer/FileViewer";
import { useProjectBrowserState } from "./browserState";
import { setGitModeWorktree } from "./gitModeWorktree";
import { FileFilterField, useFileFilterState } from "./FileFilterField";
import { BrowserViewControls } from "./BrowserViewControls";
import { FilesPanel } from "./files/FilesPanel";
import { useFilesMode } from "./files/useFilesMode";
import { GitView } from "./git/GitView";
import { RowPathScope } from "./RowActions";
import { useGitReview } from "./git/useGitReview";

/** How long the header's Refresh icon spins at least, so a refresh that settles at once still reads
 * as having happened: half a turn of the slow spin (2 s per turn). The icon looks the same every
 * half turn, so stopping there does not visibly jump. */
const MIN_SPIN_MS = 1000;

/**
 * A project's browser, the content of the Files panel (`panels/panelRegistry.tsx`) in the right pane
 * while the project owns it (`layout/asideOwner.ts`), in two
 * modes: Files, the project's files live from its directory, and Git, the changes of one of its
 * repository's branches, uncommitted and committed (`useGitReview`); the read-only viewer opens
 * over any of them. It reads only the project, bar the selected session,
 * which the Git mode marks the branches of that it created and, when it is one of this project's,
 * follows in its selector.
 * The caller mounts one per project (keyed by its id), so nothing read for one project can land in
 * another's; what belongs to the project across mounts — the mode, the expanded directories, the
 * selections — is in `browserState.ts`. Only the mode on screen is refreshed on its own.
 *
 * This is the shell: the tabs, the header, the refresh and the viewers. The Files mode is
 * `useFilesMode` and `FilesPanel`, the Git mode `useGitReview` and `GitView`.
 */
export function ProjectBrowser({
  project,
  active,
  selectedSessionId,
  switching = false,
}: {
  project: Project;
  /** Whether the browser is on screen, docked or floating in; only then is it refreshed on its own. */
  active: boolean;
  /** The session selected in the sidebar: the Git mode's branch lists mark the branches it
   * created, and its newest one fills the Git mode's selector. Nothing else here reads a
   * session. */
  selectedSessionId: string | undefined;
  /** The browser appears in a right pane that is already on screen, because the user switched
   * projects: it fades in where it stands. Read once, as it mounts, so a pane that docks or floats
   * in with it, or one it stays in, plays nothing. */
  switching?: boolean;
}): React.ReactElement {
  const t = useT();
  const [fadesIn] = useState(switching);
  const browser = useProjectBrowserState(project.id);
  const mode = browser.mode;

  const filters = { files: useFileFilterState(), changes: useFileFilterState() };
  const tab = mode === "files" ? "files" : "changes";
  const activeFilter = filters[tab];
  const files = useFilesMode(project.id, browser, filters.files, active, mode === "files");
  // The selected session sets the Git mode's selector, when it is one of this project's.
  const sessionHere = useDaemonStore((s) => (selectedSessionId !== undefined && s.sessions.get(selectedSessionId)?.project_id === project.id ? selectedSessionId : undefined));
  const git = useGitReview(project.id, browser, active && mode === "git", sessionHere, filters.changes);
  // Where a new shell terminal of the project starts: the worktree the Git mode shows.
  useEffect(() => setGitModeWorktree(project.id, mode === "git" ? git.worktree : undefined), [project.id, mode, git.worktree]);
  const refreshRef = useRef<HTMLButtonElement>(null);
  const partialMarkId = useId();
  // The hand-off covers the header's controls too: Collapse all disables itself as it is pressed,
  // which blurs it to `<body>`, and it hands focus to Refresh beside it.
  const filesFocus = useFocusHandoff(() => refreshRef.current?.focus());
  useEffect(() => browser.touch(), []);

  // A document link replaces the file on screen. Closing the change viewer here does not move
  // focus to its row: that would take focus out of the file viewer that is about to show.
  const openLinkedFile = (path: string, fragment?: string) => {
    git.dismissViewer();
    browser.setMode("files");
    files.openFile(path, fragment);
  };

  const refresh = () => (mode === "files" ? files.refresh() : git.refresh());

  const branch = useDaemonStore((state) => {
    const status = state.gitStatuses.get(project.id);
    return status ? `${status.detached}:${status.branch ?? ""}` : undefined;
  });
  const refreshedBranch = useRef({ files: branch, git: branch });
  useEffect(() => {
    if (!active || refreshedBranch.current[mode] === branch) return;
    refreshedBranch.current[mode] = branch;
    refresh();
  }, [active, branch, mode]);

  // The Refresh icon spins while a refresh the button started is out, and for at least
  // `MIN_SPIN_MS`. Only the button's own refreshes count: the periodic ones would keep it turning.
  // Pressing again while it spins starts another refresh, and the icon spins until the last settles.
  const [spinning, setSpinning] = useState(0);
  const pressRefresh = () => {
    setSpinning((n) => n + 1);
    void Promise.all([refresh(), new Promise((resolve) => setTimeout(resolve, MIN_SPIN_MS))]).finally(() => setSpinning((n) => n - 1));
  };

  return (
    <>
      <div className={`${fadesIn ? "panel-switch " : ""}relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden`}>
        <Tabs
          selectedKey={tab}
          onSelectionChange={(key) => browser.setMode(key === "changes" ? "git" : "files")}
          className="flex min-h-0 flex-1 flex-col gap-0"
        >
          <PanelHeader
            prioritizeChildren
            title={project.name}
            titleMark={mode === "files" && !files.searching && files.root?.state === "loaded" && !files.root.complete ? (
              <>
                {/* The mark goes when the root lists completely; focus it or its popover held goes to the header's first control. */}
                <HandOffOnUnmount from={`[data-status-mark-id="${partialMarkId}"]`} to={() => handFocusToRegionStart("panel-files")} />
                <StatusMark tone="warning" markId={partialMarkId} label={t("browser.partial")} />
              </>
            ) : undefined}
            actions={
              <div className="contents" onFocus={filesFocus.onFocus} onBlur={filesFocus.onBlur}>
                <BrowserViewControls
                  filter={activeFilter}
                  collapseAll={mode === "files" ? { disabled: !files.canCollapseAll, onPress: files.collapseAll } : undefined}
                  layout={mode === "git" ? { value: browser.layout, onChange: browser.setLayout } : undefined}
                />
                <TitledControl title={t("browser.refresh")}>
                  <Button ref={refreshRef} isIconOnly size="xs" variant="ghost" className={ROW_ICON_BUTTON_CLASS} aria-label={t("browser.refresh")} preventFocusOnPress onPress={pressRefresh}>
                    <RefreshCw aria-hidden="true" className={spinning > 0 ? "size-4 motion-safe:animate-spin-slow" : "size-4"} />
                  </Button>
                </TitledControl>
              </div>
            }
          >
            <LocalizedTabListContainer className="shrink-0 rounded-none bg-transparent">
              <Tabs.List aria-label={t("browser.modes")} className={TEXT_TAB_LIST_CLASS}>
                <Tabs.Tab id="files" className={`h-6 px-1.5 ${TEXT_TAB_BASE_CLASS}`}>{t("browser.mode.files")}<Tabs.Indicator /></Tabs.Tab>
                <Tabs.Tab id="changes" className={`h-6 px-1.5 ${TEXT_TAB_BASE_CLASS}`}>{t("git.view.changes")}<Tabs.Indicator /></Tabs.Tab>
              </Tabs.List>
            </LocalizedTabListContainer>
          </PanelHeader>
          <FileFilterField contextKey={tab} state={activeFilter} ignoredToggle={mode === "files"} incomplete={mode === "files" && files.searchIncomplete} label={t("browser.filter.label")} placeholder={t(mode === "files" ? "browser.filter.placeholder" : "git.filter.placeholder")} />
          {mode === "files" ? <Tabs.Panel key="files" id="files" className="mt-0 flex min-h-0 flex-1 flex-col p-0">
            <FilesPanel project={project.id} files={files} refreshRef={refreshRef} focusHandoff={filesFocus} />
          </Tabs.Panel> : <Tabs.Panel key="changes" id="changes" className="mt-0 flex min-h-0 flex-1 flex-col p-0">
            <RowPathScope.Provider value={{ project: project.id, worktree: git.worktree }}>
              <GitView
                {...git.view}
                selectedSessionId={selectedSessionId}
                onRetry={(from) => {
                  // The failure and its button go as the request starts again; focus on it moves to
                  // the header's Refresh, which stays.
                  handFocusOff(from, refreshRef.current, true);
                  refresh();
                }}
              />
            </RowPathScope.Provider>
          </Tabs.Panel>}
        </Tabs>
      </div>
      {files.viewer && (
        <FileViewer
          subject={files.viewer.subject}
          onClose={files.viewer.onClose}
          navigation={files.viewer.navigation}
          location={{
            project: project.id,
            anchor: files.viewer.anchor,
            onOpenFile: openLinkedFile,
            isDirectory: files.isDirectory,
          }}
        />
      )}
      {git.viewer && (
        <FileViewer
          {...git.viewer}
          location={{
            project: project.id,
            onOpenFile: openLinkedFile,
            isDirectory: files.isDirectory,
          }}
        />
      )}
    </>
  );
}
