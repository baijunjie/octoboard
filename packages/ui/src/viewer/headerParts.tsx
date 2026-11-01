import { ChevronLeft, ChevronRight, FolderOpen, SquareArrowOutUpRight } from "lucide-react";
import React from "react";

import { Button } from "../components/controls/Button";
import { CopyButton } from "../components/controls/CopyButton";
import { FileTypeIcon } from "../components/icons/FileTypeIcon";
import { IndicatorTooltip } from "../components/hint/IndicatorTooltip";
import { ROW_ICON_BUTTON_CLASS } from "../components/controls/RowControls";
import { TitledControl } from "../components/hint/TitledControl";
import { PlainMarkedPath } from "../components/text/MarkedPath";
import { type HostPath, useHostPathActions } from "../components/host/useHostPathActions";
import { Message, useT } from "../i18n/react";
import { displayWirePath } from "../wirePath";
import { ChangeStatusBadge } from "./ChangeStatusBadge";
import type { ViewerChange } from "./content";
import type { StatusKey } from "./statusMarks";

export function NavigationButton({
  ref,
  label,
  onPress,
  icon,
  onFocus,
  onBlur,
}: {
  ref: React.Ref<HTMLButtonElement>;
  label: string;
  onPress: (() => void) | undefined;
  icon: "previous" | "next";
  onFocus: () => void;
  onBlur: (event: React.FocusEvent) => void;
}): React.ReactElement {
  const Icon = icon === "previous" ? ChevronLeft : ChevronRight;
  return (
    <TitledControl title={label}>
      <Button
        ref={ref}
        isIconOnly
        size="xs"
        variant="ghost"
        className={ROW_ICON_BUTTON_CLASS}
        aria-label={label}
        isDisabled={!onPress}
        onPress={onPress}
        onFocus={onFocus}
        onBlur={onBlur}
      >
        <Icon aria-hidden="true" className="size-4 rtl:-scale-x-100" />
      </Button>
    </TitledControl>
  );
}

/** Copy path leads the view controls at the detail row's end, an icon button that
 * takes no focus from a press, as the view controls do. */
export function CopyPathButton({ path }: { path: string }): React.ReactElement {
  const t = useT();
  return (
    <CopyButton
      text={displayWirePath(path)}
      beforeCopyText={t("common.copyPath")}
      afterCopyText={t("common.pathCopied")}
      failedText={t("common.copyFailed")}
      size="xs"
      variant="ghost"
      className={ROW_ICON_BUTTON_CLASS}
      preventFocusOnPress
    />
  );
}

/** File manager and Default application, the last of the header's controls: the same two actions, with
 * the same icons and names, as the project pane's row menu has, on the subject's live file. Each is
 * an icon button that takes no focus from a press, like the others. */
export function HostPathButtons({ host }: { host: HostPath }): React.ReactElement {
  const t = useT();
  const act = useHostPathActions();
  // The tooltip is the short label the row menu has; the accessible name says what pressing does.
  const buttons = [
    { type: "reveal_project_path", title: t("common.fileManager"), name: t("viewer.openInFileManager"), Icon: FolderOpen },
    { type: "open_project_path", title: t("common.defaultApplication"), name: t("viewer.openInDefaultApplication"), Icon: SquareArrowOutUpRight },
  ] as const;
  return (
    <>
      {buttons.map(({ type, title, name, Icon }) => (
        <TitledControl key={type} title={title}>
          <Button isIconOnly size="xs" variant="ghost" className={ROW_ICON_BUTTON_CLASS} aria-label={name} onPress={() => act(type, host)} preventFocusOnPress>
            <Icon aria-hidden="true" className="size-4" />
          </Button>
        </TitledControl>
      ))}
    </>
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
