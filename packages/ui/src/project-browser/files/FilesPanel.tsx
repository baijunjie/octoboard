import { FolderX, SearchX } from "lucide-react";
import React from "react";

import { StatusAnnouncer } from "../../components/announce/StatusAnnouncer";
import { PaneFailure } from "../../components/alerts/PaneFailure";
import { handFocusOff } from "../../components/focus/handFocusOff";
import type { useFocusHandoff } from "../../components/focus/useFocusHandoff";
import { useHasPainted } from "../../components/motion/useHasPainted";
import { CenteredEmptyPanel } from "../../components/panel/EmptyPanel";
import { LoadingState } from "../../components/panel/LoadingState";
import { useT } from "../../i18n/react";
import { RowPathScope } from "../RowActions";
import { FileTree } from "./FileTree";
import type { FilesMode } from "./useFilesMode";

/** Gives `children` whether the Files panel it mounts with has painted, so a failure that appears
 * once it has is announced and one the panel opened with is not. Switching back to the panel mounts
 * a fresh one. */
function WhenPainted({ children }: { children: (painted: boolean) => React.ReactNode }): React.ReactElement {
  return <>{children(useHasPainted())}</>;
}

/**
 * What the Files mode shows under the header: the project's tree, or the filter's results as one,
 * or why there is none yet (still loading, failed, nothing to list). A failure's Try again hands
 * keyboard focus to `refreshRef`, the header's Refresh, since the failure and its button go as the
 * request starts again.
 */
export function FilesPanel({
  project,
  files,
  refreshRef,
  focusHandoff,
}: {
  project: string;
  files: FilesMode;
  refreshRef: React.RefObject<HTMLElement | null>;
  /** The focus hand-off of the header's controls, which covers this panel as well. */
  focusHandoff: ReturnType<typeof useFocusHandoff>;
}): React.ReactElement {
  const t = useT();
  const { searching, searchFailure, root, tree } = files;
  const loadingLabel = t(searching ? "browser.filter.searching" : "browser.loading");
  return (
    <WhenPainted>
      {(painted) => (
        <div className="flex min-h-0 flex-1 flex-col" onFocus={focusHandoff.onFocus} onBlur={focusHandoff.onBlur}>
          <StatusAnnouncer text={files.rootLoading ? loadingLabel : undefined} />
          {files.rootLoading ? (
            <LoadingState label={loadingLabel} />
          ) : searchFailure ? (
            <PaneFailure title={t("browser.filter.error")} message={searchFailure.message} announce={painted} onRetry={(from) => { handFocusOff(from, refreshRef.current, true); files.retrySearch(); }} />
          ) : !searching && root?.state === "error" ? (
            <PaneFailure
              title={t("browser.error.root")}
              message={root.message}
              announce={painted}
              onRetry={(from) => {
                // The error and its button go as the listing starts again; focus on it moves to the
                // header's Refresh, which stays.
                handFocusOff(from, refreshRef.current, true);
                files.retryDirectory("");
              }}
            />
          ) : tree.nodes.length === 0 && (searching || root?.state === "loaded" && root.complete) ? (
            <CenteredEmptyPanel icon={searching ? SearchX : FolderX} message={t(searching ? "browser.filter.noMatch" : "browser.empty")} />
          ) : (
            <RowPathScope.Provider value={{ project }}>
              <FileTree
                nodes={tree.nodes}
                expanded={files.expandedKeys}
                label={t("browser.tree.label")}
                onExpandedChange={files.onExpandedChange}
                onOpenFile={files.openFile}
                onRetry={files.retryDirectory}
                treeRef={files.treeRef}
              />
            </RowPathScope.Provider>
          )}
        </div>
      )}
    </WhenPainted>
  );
}
