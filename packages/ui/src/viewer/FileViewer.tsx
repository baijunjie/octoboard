import React, { useMemo, useRef, useState } from "react";

import { ChangeLineCounts } from "../components/status/ChangeLineCounts";
import { FadeOverflow } from "../components/text/FadeOverflow";
import { FileTypeIcon } from "../components/icons/FileTypeIcon";
import { PlainMarkedPath } from "../components/text/MarkedPath";
import { RowIconGroup } from "../components/controls/RowControls";
import { StatusBadge } from "../components/status/StatusBadge";
import { Dialog } from "../components/dialog/Dialog";
import { useCurrentLanguage, useT } from "../i18n/react";
import { minWidthQuery, useMediaQuery } from "../components/hooks/breakpoint";
import { displayWirePath, wireBaseName } from "../format/wirePath";
import { useRendererScope } from "./code/codeRenderer";
import { changeStatus, type ViewerSubject } from "./content";
import { ControlsSlot } from "./controlsSlot";
import { diffLayout } from "./code/diffLayout";
import { FindBar, FindToggle } from "./find/FindBar";
import {
  CopyPathButton,
  DetailItem,
  HostPathButtons,
  NavigationButton,
  renamedFromPath,
  RenamedFrom,
  TitleStatusBadge,
} from "./headerParts";
import { formatFileSize } from "../format/fileSize";
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
}: {
  subject: ViewerSubject;
  onClose: () => void;
  navigation?: ViewerNavigation;
  /** The line of a file to scroll to and mark; ignored for a change and for an image. */
  line?: number;
  /** How a link or an image in a document is followed. The document's path is the subject's. */
  location?: Omit<MarkdownLocation, "path">;
}): React.ReactElement {
  const t = useT();
  const fullScreen = !useMediaQuery(() => minWidthQuery("phone"));
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
  const name = wireBaseName(subject.path);
  const { content } = subject;
  useRendererScope();

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
      <NavigationButton
        ref={previousRef}
        label={t("viewer.previous")}
        onPress={navigation.onPrevious}
        icon="previous"
        onFocus={() => focusNavigation("previous")}
        onBlur={(event) => blurNavigation("previous", event)}
      />
      <NavigationButton
        ref={nextRef}
        label={t("viewer.next")}
        onPress={navigation.onNext}
        icon="next"
        onFocus={() => focusNavigation("next")}
        onBlur={(event) => blurNavigation("next", event)}
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
        <PlainMarkedPath path={displayWirePath(subject.path)} icon={<FileTypeIcon path={subject.path} className="size-3.5 shrink-0 text-app-muted" />} />
        <NoticeSlotBoxes />
      </DetailItem>
      {subject.stats && subject.stats.added + subject.stats.removed > 0 && (
        <DetailItem className="shrink-0">
          <ChangeLineCounts stats={subject.stats} />
        </DetailItem>
      )}
      {content.state === "file" && (
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
        <div ref={headerRef} data-viewer-description="" className="flex min-w-0 shrink-0 flex-col groove-b px-6 max-phone:px-3 text-xs leading-5">
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
              <CopyPathButton path={subject.path} />
              <div ref={setMarkdownViewSlot} className="contents" />
              <div ref={setLayoutSlot} className="contents" />
              {wrapClaimed && <WrapToggleHost />}
              {subject.host && <HostPathButtons host={subject.host} />}
            </RowIconGroup>
          </div>
        </div>
        {/* The find bar floats over the content's top trailing corner; while it is open the scrolled text
            starts below it (`data-find-open` in style.css), so it covers no line until the reader scrolls.
            The content is in a box of its own so the bar's changes do not count as the drawn text changing. */}
        <div className="relative flex min-h-0 flex-1 flex-col" data-find-open={find.bar ? "" : undefined}>
          {find.bar && <FindBar {...find.bar} />}
          <div ref={contentRef} data-find-content="" className="flex min-h-0 flex-1 flex-col gap-3">
            <ControlsSlot value={controls}>
              <MarkdownLocationProvider value={{ path: subject.path, ...location }}>
                <ViewerContentView subject={subject} name={name} layout={layout} onLayoutChange={diffLayout.set} line={line} />
              </MarkdownLocationProvider>
            </ControlsSlot>
          </div>
        </div>
      </NoticeSlotsProvider>
    </Dialog>
  );
}

