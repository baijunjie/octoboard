import React, { useMemo, useState } from "react";

import { ChangeLineCounts } from "../components/status/ChangeLineCounts";
import { FadeOverflow } from "../components/text/FadeOverflow";
import { FileTypeIcon } from "../components/icons/FileTypeIcon";
import { PlainMarkedPath } from "../components/text/MarkedPath";
import { RowIconGroup } from "../components/controls/RowControls";
import { StatusBadge } from "../components/status/StatusBadge";
import { Dialog } from "../components/dialog/Dialog";
import { useCurrentLanguage, useT } from "../i18n/react";
import { displayWirePath, wireBaseName } from "../wirePath";
import { useRendererScope } from "./codeRenderer";
import { changeStatus, type ViewerSubject } from "./content";
import { ControlsSlot } from "./controlsSlot";
import { diffLayout } from "./diffLayout";
import {
  CopyPathButton,
  DetailItem,
  HostPathButtons,
  NavigationButton,
  renamedFromPath,
  RenamedFrom,
  TitleStatusBadge,
} from "./headerParts";
import { formatFileSize } from "./format";
import { MarkdownLocationProvider, type MarkdownLocation } from "./markdownLocation";
import { NoticeSlotBoxes, NoticeSlotsProvider } from "./notices";
import { STATUS_LABELS, type StatusKey } from "./statusMarks";
import { useNavigationFocus } from "./useNavigationFocus";
import { ViewerContentView } from "./ViewerContentView";
import type { ViewerNavigation } from "./viewerNavigation";
import { useWrapClaims, WrapToggleHost } from "./wordWrap";

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

  // The header holds navigation beside the title, the file's name with the tags of its change; under
  // it, when the subject has one, the subtitle (where a compared change is from, which is a tag in
  // meaning but far too long to sit in the tag group); and under that, in the body, the details
  // (path, size, rename origin) beside the controls (Copy path, then the Markdown view, diff layout
  // and word wrap choices), which float at the row's end. The details are separate items laid out like
  // words around that float (`DetailItem`): they share the row with the controls to use the width
  // beside them, and one too long for the space left moves down to a row of its own, where it has
  // the whole width, rather than being squeezed against the others. Each item is one line cut by
  // a fade, so only a detail that does not fit even alone is clipped.
  // The block is the row beside the controls plus a row for each further line the dropped items
  // need (short items that all miss the space beside the controls share one): two rows when a long
  // path drops, three when a rename's long origin drops too, each taking 32 px of the code area
  // from the dialog. That is accepted, because the milestone asks that a long detail take a row of
  // its own rather than be squeezed, and capping it would squeeze one. The subtitle is a single line
  // in the dialog's header, directly under the heading (`Dialog`'s `subtitle`), for a compared
  // change and nothing for any other subject. Its height follows its text, and since the header
  // is not the scroller it is not clipped by it.
  // The notices about the whole file or change are marks right after the path, in one slot per tone so
  // that they stand failed, then warned, then notes, whichever surface raises them (`FileNotice`); they
  // stay outside the path's own fade, which cuts only the path.
  // The header also gives up what it held before, each row one line that is never wrapped, so that
  // moving between files could not move the code below; the code now moves when the rows needed
  // differ from one file to the next, and between a compared change and a worktree one.
  // An untracked file is a change with an absent old side, which `changeStatus` calls added; the
  // list marks it untracked and says so in `subject.status`, which also holds while the content is
  // not read (yet). Without one, a change's sides give it, and a path in conflict has none to compare.
  const status: StatusKey | undefined =
    subject.status ?? (content.state === "change" ? changeStatus(content.change) : content.state === "conflict" ? "conflicted" : undefined);
  const renamedFrom = content.state === "change" ? renamedFromPath(content.change) : undefined;
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
              className="min-w-0"
              titleWhenClipped={subject.source}
            >
              {subject.source}
            </FadeOverflow>
          </div>
        ) : undefined
      }
      onClose={onClose}
      footer={null}
      resetKey={subject.key}
    >
      <NoticeSlotsProvider>
        <div ref={headerRef} data-viewer-description="" className="flow-root min-h-8 min-w-0 shrink-0 groove-b px-6 text-xs leading-8">
          {/* Copy path, then fixed places for the Markdown view choice, the layout choice and the
            wrap choice, then File manager and Default application, so Tab order matches the screen
            whichever mounts first. The marks after the path come after those controls in the tab
            order, for the same reason the controls come before the path in the source.
            Copy path is here, not beside the path it copies, because the group sits at the row's end
            and a button at the row's start would be tabbed to after the three at its end, against the
            reading direction. The slots have no box of their own while empty, so they take none of
            the gap. The group floats to the end of the row, ahead of the details in the source so
            the line next to it is the one it shortens (a float shortens only the lines that follow
            it). That puts the controls before the file's path for a screen reader, so Copy path is
            met before the path it copies. That cost was weighed and accepted: the details are
            independent, separately labelled facts, and Copy path and the other controls are labelled
            on their own. */}
          <RowIconGroup className="float-end h-8">
            <CopyPathButton path={subject.path} />
            <div ref={setMarkdownViewSlot} className="contents" />
            <div ref={setLayoutSlot} className="contents" />
            {wrapClaimed && <WrapToggleHost />}
            {subject.host && <HostPathButtons host={subject.host} />}
          </RowIconGroup>
          <DetailItem>
            <PlainMarkedPath path={displayWirePath(subject.path)} icon={<FileTypeIcon path={subject.path} className="size-3.5 shrink-0 text-app-muted" />} />
            <NoticeSlotBoxes />
          </DetailItem>
          {subject.stats && subject.stats.added + subject.stats.removed > 0 && (
            <DetailItem>
              <ChangeLineCounts stats={subject.stats} />
            </DetailItem>
          )}
          {content.state === "file" && (
            <DetailItem>
              <StatusBadge separated={false}>
                {formatFileSize(language, content.body.size)}
              </StatusBadge>
            </DetailItem>
          )}
          {renamedFrom !== undefined && <RenamedFrom path={renamedFrom} />}
        </div>
        <div className="flex min-h-0 flex-1 flex-col gap-3">
          <ControlsSlot value={controls}>
            <MarkdownLocationProvider value={{ path: subject.path, ...location }}>
              <ViewerContentView subject={subject} name={name} layout={layout} onLayoutChange={diffLayout.set} line={line} />
            </MarkdownLocationProvider>
          </ControlsSlot>
        </div>
      </NoticeSlotsProvider>
    </Dialog>
  );
}

