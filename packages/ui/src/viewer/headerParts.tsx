import { Download, FolderOpen, SquareArrowOutUpRight } from "lucide-react";
import React from "react";

import { CopyButton } from "../components/controls/CopyButton";
import { FileTypeIcon } from "../components/icons/FileTypeIcon";
import { IndicatorTooltip } from "../components/hint/IndicatorTooltip";
import { ROW_ICON_BUTTON_CLASS } from "../components/controls/RowControls";
import { TitledControl } from "../components/hint/TitledControl";
import { PlainMarkedPath } from "../components/text/MarkedPath";
import { type HostPath, useHostPathActions } from "../components/host/useHostPathActions";
import { useDaemon } from "../store";
import { Message, useT } from "../i18n/react";
import { displayWirePath } from "../format/wirePath";
import { openPathInDefaultApplication } from "../components/host/openPathInDefaultApplication";
import { revealPathTarget } from "../components/host/revealPathTarget";
import { ChangeStatusBadge } from "./ChangeStatusBadge";
import type { ViewerChange } from "./content";
import type { FileDownload } from "./download/useFileDownload";
import type { StatusKey } from "./statusMarks";
import { Button } from "../primitives";

/** Copy path leads the view controls at the detail row's end, an icon button that
 * takes no focus from a press, as the view controls do. It copies `text`, the path as the header shows it. */
export function CopyPathButton({ text }: { text: string }): React.ReactElement {
  const t = useT();
  return (
    <CopyButton
      text={text}
      beforeCopyText={t("common.copyPath")}
      afterCopyText={t("common.pathCopied")}
      failedText={t("common.copyFailed")}
      size="xs"
      variant="ghost"
      className={ROW_ICON_BUTTON_CLASS}
      focusOnPress={false}
    />
  );
}

/** What opening the live file in its default application does, on the daemon's host: for a file of a project the row's
 * action, for one only a terminal's path link names the path link rules'. Shared by the header's Default application
 * and the stand-in's Open in default application, whose press runs it. Both refuse what the system could launch. */
export function useOpenInDefaultApplication({ host, linkHost }: { host?: HostPath; linkHost?: { project?: string; path: string } }): () => void {
  const act = useHostPathActions();
  const { request } = useDaemon();
  return () => (linkHost ? void openPathInDefaultApplication({ path: linkHost.path, project: linkHost.project, request }) : host && act("open_project_path", host));
}

/** File manager and Default application, the last of the header's controls: the same two actions, with
 * the same icons and names, as the project pane's row menu has, on the subject's live file. Each is
 * an icon button that takes no focus from a press, like the others. A file of a project is acted on
 * by `host`; one only a terminal's path link names, by `linkHost`, through the path link rules. Default application
 * (`openDefault`, which the viewer holds for its stand-in as well) is left out for a `launchable` file, which the daemon
 * would refuse to open. In a remote window, whose daemon's
 * machine is not this computer's, they give way to Download and open (`download`), where the window can copy a file
 * here, and there are none where it cannot. */
export function HostPathButtons({ host, linkHost, launchable = false, download, openDefault }: { host?: HostPath; linkHost?: { project?: string; path: string }; launchable?: boolean; download?: FileDownload; openDefault: () => void }): React.ReactElement | null {
  const t = useT();
  const act = useHostPathActions();
  // Both are this machine's: not offered for a daemon elsewhere.
  const { request, isLocal } = useDaemon();
  if (isLocal === false) return download ? <DownloadButton download={download} /> : null;
  // The tooltip is the short label the row menu has; the accessible name says what pressing does.
  const buttons = [
    {
      key: "reveal",
      title: t("common.fileManager"),
      name: t("viewer.openInFileManager"),
      Icon: FolderOpen,
      run: () => (linkHost ? void revealPathTarget({ path: linkHost.path, project: linkHost.project, request }) : host && act("reveal_project_path", host)),
    },
    ...(launchable
      ? []
      : [{
        key: "open",
        title: t("common.defaultApplication"),
        name: t("viewer.openInDefaultApplication"),
        Icon: SquareArrowOutUpRight,
        run: openDefault,
      }]),
  ];
  return (
    <>
      {buttons.map(({ key, title, name, Icon, run }) => (
        <TitledControl key={key} title={title}>
          <Button iconOnly size="xs" variant="ghost" className={ROW_ICON_BUTTON_CLASS} label={name} onPress={run} focusOnPress={false}>
            <Icon className="size-4" />
          </Button>
        </TitledControl>
      ))}
    </>
  );
}

/** Download and open in a remote window's header, where File manager and Default application stand locally: an icon
 * button that takes no focus from a press and is busy while the copy is made, the stand-in showing how far it has come. */
function DownloadButton({ download }: { download: FileDownload }): React.ReactElement {
  const t = useT();
  return (
    <TitledControl title={t("viewer.downloadAndOpen")}>
      <Button iconOnly size="xs" variant="ghost" className={ROW_ICON_BUTTON_CLASS} label={t("viewer.downloadAndOpen")} pending={download.progress !== undefined} onPress={download.start} focusOnPress={false}>
        <Download className="size-4" />
      </Button>
    </TitledControl>
  );
}

/** For a rename within the project, the path it came from. */
export function renamedFromPath(change: ViewerChange): string | undefined {
  return change.old.state === "present" && change.new.state === "present" && change.old.path !== change.new.path ? change.old.path : undefined;
}

/** One detail of the header, with paths allowed to shrink beside their fixed metadata. */
export function DetailItem({ children, className = "" }: { children: React.ReactNode; className?: string }): React.ReactElement {
  return <span className={`inline-flex h-5 max-w-full items-center gap-1.5 whitespace-nowrap text-app-muted ${className}`}>{children}</span>;
}

export function RenamedFrom({ path }: { path: string }): React.ReactElement {
  return (
    <span className="flex h-5 min-w-0 max-w-full items-center gap-1.5 whitespace-nowrap text-app-muted">
      <Message id="viewer.change.renamedFrom" params={{ path: <PlainMarkedPath path={displayWirePath(path)} icon={<FileTypeIcon path={path} className="size-3.5 shrink-0 text-app-muted" />} /> }} />
    </span>
  );
}

/** The title's status badge, with a tooltip where there is more to say than its word (a conflict's kind):
 * the indicator kind, which a tap on a touch screen opens as well, since a tap on the badge does nothing
 * else. It is a `span` inside the heading, which is no place for a `div`, and takes no name: the drawn
 * title is hidden from assistive technology, which reads the same sentence after the heading's name. */
export function TitleStatusBadge({ status, label, tooltip }: { status: StatusKey; label: string; tooltip?: string }): React.ReactElement {
  const badge = <ChangeStatusBadge status={status}>{label}</ChangeStatusBadge>;
  if (tooltip === undefined) return badge;
  return (
    <IndicatorTooltip tooltip={tooltip} phrasing className="inline-flex">
      {badge}
    </IndicatorTooltip>
  );
}
