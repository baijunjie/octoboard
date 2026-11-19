import React, { useMemo, useRef, useState } from "react";

import { ChangeLineCounts } from "../components/status/ChangeLineCounts";
import { FadeOverflow } from "../components/text/FadeOverflow";
import { FileTypeIcon } from "../components/icons/FileTypeIcon";
import { PlainMarkedPath } from "../components/text/MarkedPath";
import { PagerButton } from "../components/controls/PagerButton";
import { RowIconGroup } from "../components/controls/RowControls";
import { StatusBadge } from "../components/status/StatusBadge";
import { Dialog } from "../components/dialog/Dialog";
import { useCurrentLanguage, useT } from "../i18n/react";
import { useHost } from "../platform/react";
import type { RangeFile } from "../protocol";
import { useDaemon } from "../store";
import { useIsNarrow } from "../components/hooks/breakpoint";
import { basename, display } from "../format/hostPath";
import { displayWirePath, wireBaseName } from "../format/wirePath";
import { useHostPathsOrEmpty } from "../components/hooks/useHostPaths";
import { useRendererScope } from "./code/codeRenderer";
import { changeStatus, type ViewerSubject } from "./content";
import { ControlsSlot } from "./controlsSlot";
import { diffLayout } from "./code/diffLayout";
import type { LineRange } from "./code/lineMarks";
import { FindBar, FindToggle } from "./find/FindBar";
import {
  CopyPathButton,
  DetailItem,
  HostPathButtons,
  renamedFromPath,
  useOpenInDefaultApplication,
  RenamedFrom,
  TitleStatusBadge,
} from "./headerParts";
import { formatFileSize } from "../format/fileSize";
import { isLaunchableName } from "../launchable";
import { useFileDownload } from "./download/useFileDownload";
import { StandInActionContext, type StandInAction } from "./standInAction";
import { MarkdownLocationProvider, type MarkdownLocation } from "./markdown/markdownLocation";
import { NoticeSlotBoxes, NoticeSlotsProvider } from "./notices";
import { STATUS_LABELS, type StatusKey } from "./statusMarks";
import { useFind } from "./find/useFind";
import { useNavigationFocus } from "./useNavigationFocus";
import { isWaiting, ViewerContentView } from "./ViewerContentView";
import type { ViewerNavigation } from "./viewerNavigation";
import { useWrapClaims, WrapToggleHost } from "./code/wordWrap";

/**
 * The read-only file viewer: a modal showing one subject — a file's text or image, a change between
 * two versions, or the loading or failed state of either. It stays mounted while its subject
 * changes (`subject.key`), so moving between files keeps focus inside it; content and errors are
 * the subject's own and never outlive it. It offers no editing of any kind.
 *
 * Left and Right do what Previous and Next do (`arrowNavigation` has when they do not), handled on
 * the dialog itself: a key pressed in it never reaches anything outside, the terminal included.
 */
export function FileViewer({
  subject,
  onClose,
  navigation,
  location,
  line,
  marks,
}: {
  subject: ViewerSubject;
  onClose: () => void;
  navigation?: ViewerNavigation;
  /** The line of a file to scroll to and mark; ignored for a change and for an image. */
  line?: number;
  /** Ranges of `line`'s text to mark, as a match of a content search, for as long as the file is shown at that line. */
  marks?: readonly LineRange[];
  /** How a link or an image in a document is followed. The document's path is the subject's. */
  location?: Omit<MarkdownLocation, "path">;
}): React.ReactElement {
  const t = useT();
  const fullScreen = useIsNarrow();
  const language = useCurrentLanguage();
  // The user's last choice, kept across files and restarts (`diffLayout`).
  const layout = diffLayout.useValue();
  const [markdownViewSlot, setMarkdownViewSlot] = useState<HTMLElement | null>(null);
  const [layoutSlot, setLayoutSlot] = useState<HTMLElement | null>(null);
  const { claimWrap, claimed: wrapClaimed } = useWrapClaims();
  const controls = useMemo(
    () => ({ markdownView: markdownViewSlot, layout: layoutSlot, claimWrap }),
    [markdownViewSlot, layoutSlot, claimWrap],
  );
  // A file a path link names is shown by the path as the daemon's host spells it, not a project-relative wire path.
  const hostPaths = useHostPathsOrEmpty();
  const shownPath = subject.linkHost ? display(subject.path, hostPaths) : displayWirePath(subject.path);
  const name = subject.linkHost ? basename(subject.path, hostPaths.style) : wireBaseName(subject.path);
  const { content } = subject;
  useRendererScope();
  // What the stand-in and the header offer for a live file, outside the viewer. One copy at a time, shared by both.
  const { isLocal } = useDaemon();
  const file: RangeFile | undefined = subject.host
    ? { kind: "project", project: subject.host.project, ...(subject.host.worktree !== undefined && { worktree: subject.host.worktree }), path: subject.host.path }
    : subject.linkHost
      ? { kind: "path_link", ...(subject.linkHost.project !== undefined && { project: subject.linkHost.project }), path: subject.linkHost.path }
      : undefined;
  // Where the file is opened decides what launches it. Default application opens it on the daemon's host, whose own word
  // (`subject.launchable`) covers its executable bit and its lists. The copy Download and open makes opens on this
  // computer, whose system judges the name; the daemon's word counts there only where it is not just the name's (an
  // executable bit, a bundle), and the first request of the copy asks the daemon for the bit again (`downloadFile`).
  const desktop = useHost();
  const hostWindows = hostPaths.style === "windows";
  const launchable = subject.launchable === true || isLaunchableName(name, hostWindows);
  const copyLaunchable = isLaunchableName(name, desktop.os === "windows") || (subject.launchable === true && !isLaunchableName(name, hostWindows));
  const download = useFileDownload({ key: subject.key, file, name, launchable: copyLaunchable });
  const openDefault = useOpenInDefaultApplication({ host: subject.host, linkHost: subject.linkHost });
  const standInAction: StandInAction | undefined = !file
    ? undefined
    : isLocal === false
      ? download && { kind: "download", download }
      : launchable
        ? undefined
        : { kind: "open", run: openDefault };

  // Focus handling for the dialog (`useNavigationFocus`): a navigation button that turns disabled
  // under focus, the content replaced under the focused code region, and Left and Right.
  const { previousRef, nextRef, headerRef, focusNavigation, blurNavigation } = useNavigationFocus({
    navigation,
    subjectKey: subject.key,
    content,
    wrapClaimed,
  });

  // Find in file (⌘F), over the text the content draws.
  const contentRef = useRef<HTMLDivElement>(null);
  const find = useFind({ headerRef, contentRef, subjectKey: subject.key, waiting: isWaiting(content) });

  const navigationControls = navigation ? (
    <RowIconGroup>
      <PagerButton
        ref={previousRef}
        label={t("viewer.previous")}
        direction="previous"
        isDisabled={!navigation.onPrevious}
        onPress={() => navigation.onPrevious?.()}
        onFocus={() => focusNavigation("previous")}
        onBlur={(blur) => blurNavigation("previous", blur)}
      />
      <PagerButton
        ref={nextRef}
        label={t("viewer.next")}
        direction="next"
        isDisabled={!navigation.onNext}
        onPress={() => navigation.onNext?.()}
        onFocus={() => focusNavigation("next")}
        onBlur={(blur) => blurNavigation("next", blur)}
      />
    </RowIconGroup>
  ) : null;

  // An untracked file is a change with an absent old side, which `changeStatus` calls added; the
  // list marks it untracked and says so in `subject.status`, which also holds while the content is
  // not read (yet). Without one, a change's sides give it, and a path in conflict has none to compare.
  const status: StatusKey | undefined =
    subject.status ?? (content.state === "change" ? changeStatus(content.change) : content.state === "conflict" ? "conflicted" : undefined);
  const renamedFrom = content.state === "change" ? renamedFromPath(content.change) : undefined;
  // The current path, its notice marks and the line counts or size: one row, the path shrinking to fit.
  // Both paths fade at their start, preserving the file name; notices stay outside that fade, ordered
  // failed, warned, then notes.
  const fileSummary = (
    <div data-viewer-file-summary="" className="flex min-w-0 items-center gap-4">
      <DetailItem className="min-w-0">
        <PlainMarkedPath path={shownPath} icon={<FileTypeIcon path={subject.linkHost ? name : subject.path} className="size-3.5 shrink-0 text-app-muted" />} />
        <NoticeSlotBoxes />
      </DetailItem>
      {subject.stats && subject.stats.added + subject.stats.removed > 0 && (
        <DetailItem className="shrink-0">
          <ChangeLineCounts stats={subject.stats} />
        </DetailItem>
      )}
      {content.state === "file" && content.body.size !== null && (
        <DetailItem className="shrink-0">
          <StatusBadge separated={false}>
            {formatFileSize(language, content.body.size)}
          </StatusBadge>
        </DetailItem>
      )}
    </div>
  );
  const statusLabel = status ? t(STATUS_LABELS[status]) : undefined;
  const tags = [subject.stage, statusLabel].filter((tag): tag is string => tag !== undefined);
  // What kind of conflict a conflicted path is in: the status badge's tooltip, and part of the heading's
  // name, since the drawn badge is hidden from assistive technology.
  const conflictKind = content.state === "conflict" ? content.conflict : undefined;
  const tagged = tags.length > 0 ? `${name}${t("viewer.titleSeparator")}${tags.join(t("viewer.tagSeparator"))}` : name;
  const titleName = conflictKind !== undefined ? t("viewer.titleWithDetail", { title: tagged, detail: conflictKind }) : tagged;
  return (
    <Dialog
      size="viewer"
      fullScreen={fullScreen}
      headerLeading={navigationControls}
      title={
        <>
          {/* WebKit puts a space around every flex item when it computes an accessible name
              ("a.ts : Staged , Modified"), so the name is one string built here from the catalog and
              the drawn boxes are hidden from it. select-none keeps a copied heading from holding the
              text twice. */}
          <span className="sr-only select-none">{titleName}</span>
          <span aria-hidden="true" className="flex min-w-0 items-center gap-2.5">
            <FadeOverflow as="span" dir="ltr" className="min-w-0" titleWhenClipped={name}>
              {name}
            </FadeOverflow>
            {/* Where the change is from, neutral, then what it is, coloured, since a neutral qualifier goes
                before the coloured status: both tags come after the name, whichever the subject has, and never
                shrink, so the name is what the fade cuts. The tags sit closer to each other than the group
                does to the name. */}
            {tags.length > 0 && (
              <span className="flex shrink-0 items-center gap-1">
                {subject.stage !== undefined && (
                  <StatusBadge separated={false}>
                    {subject.stage}
                  </StatusBadge>
                )}
                {status && <TitleStatusBadge status={status} label={statusLabel!} tooltip={conflictKind} />}
              </span>
            )}
          </span>
        </>
      }
      subtitle={
        subject.source ? (
          // Part of the subject's identity, so it comes straight after the title in the reading order.
          // Rendered only while there is a source, so a subject without one loses no height. The muted
          // colour is set here rather than inherited: the dialog's header carries none of its own, and
          // this line is the subject's metadata, not a second title.
          <div data-viewer-subtitle="" className="col-span-full flex min-w-0 items-center text-xs text-app-muted">
            <FadeOverflow
              as="span"
              className="min-w-0 [&_code]:break-all"
              titleWhenClipped={subject.source}
            >
              {subject.source}
            </FadeOverflow>
          </div>
        ) : undefined
      }
      onClose={onClose}
      keyboardDismissHeld={find.bar !== null}
      footer={null}
      resetKey={subject.key}
    >
      <NoticeSlotsProvider>
        <div ref={headerRef} data-viewer-description="" className="flex min-w-0 shrink-0 flex-col groove-b px-6 max-tablet:px-3 text-xs leading-5">
          {/* Every details row but the last takes the whole width above. The last row shares a line with
              the controls, which end it bottom-aligned; its basis is its content width, so the controls
              wrap to a line of their own above the content only when that row does not fit beside them.
              The last row's `pb-1.5` under a `leading-5` line puts the text's centre 16 px above the
              bottom, as the controls' `h-8` does; their `-mt-0.5` lets a 30 px rename row hold them
              without growing. */}
          {renamedFrom !== undefined && <div className="pt-1.5">{fileSummary}</div>}
          <div className="flex min-w-0 flex-wrap items-end gap-x-4">
            <div data-viewer-last-details="" className={`min-w-0 flex-auto pb-1.5 ${renamedFrom !== undefined ? "pt-1" : "pt-1.5"}`}>
              {renamedFrom !== undefined ? <RenamedFrom path={renamedFrom} /> : fileSummary}
            </div>
            {/* Slots preserve their order as the Markdown, diff layout and wrapping choices mount. */}
            <RowIconGroup className="ms-auto -mt-0.5 h-8">
              {find.toggle && <FindToggle {...find.toggle} />}
              <CopyPathButton text={shownPath} />
              <div ref={setMarkdownViewSlot} className="contents" />
              <div ref={setLayoutSlot} className="contents" />
              {wrapClaimed && <WrapToggleHost />}
              {(subject.host || subject.linkHost) && <HostPathButtons host={subject.host} linkHost={subject.linkHost} launchable={launchable} download={download} openDefault={openDefault} />}
            </RowIconGroup>
          </div>
        </div>
        {/* The find bar floats over the content's top trailing corner; while it is open the scrolled text
            starts below it (`data-find-open` in style.css), so it covers no line until the reader scrolls.
            The content is in a box of its own so the bar's changes do not count as the drawn text changing. */}
        <div className="relative flex min-h-0 flex-1 flex-col bg-overlay" data-find-open={find.bar ? "" : undefined}>
          {find.bar && <FindBar {...find.bar} />}
          <div ref={contentRef} data-find-content="" className="flex min-h-0 flex-1 flex-col gap-3">
            <ControlsSlot value={controls}>
              <MarkdownLocationProvider value={{ path: subject.path, ...location }}>
                <StandInActionContext value={standInAction}>
                  <ViewerContentView subject={subject} name={name} layout={layout} onLayoutChange={diffLayout.set} line={line} marks={marks} />
                </StandInActionContext>
              </MarkdownLocationProvider>
            </ControlsSlot>
          </div>
        </div>
      </NoticeSlotsProvider>
    </Dialog>
  );
}

