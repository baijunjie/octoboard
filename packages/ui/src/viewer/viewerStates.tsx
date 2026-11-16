// Everything the viewer shows in place of the content itself: the stand-in that fills the body when
// there is nothing to draw, the error block for a subject that cannot be opened at all, and the
// spinner while any of it is on its way, with the contexts that tell them when they arrived. They are
// all here so the three stay one family. What is said about content that is still shown is in
// `notices.tsx`.
import { Download, Equal, FileQuestion, FolderGit2, Link, SquareArrowOutUpRight, Unplug } from "lucide-react";
import React, { createContext, useContext, useId, useRef, useState } from "react";

import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../components/announce/StatusAnnouncer";
import { FadeOverflow } from "../components/text/FadeOverflow";
import { useFocusHandoff } from "../components/focus/useFocusHandoff";
import { CenteredEmptyPanel } from "../components/panel/EmptyPanel";
import { LoadingState } from "../components/panel/LoadingState";
import { PaneFailure } from "../components/alerts/PaneFailure";
import type { ErrorPresentation } from "../errors/errorPresentation";
import type { PlainMessageKey } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { useCurrentLanguage, useT } from "../i18n/react";
import { Button, Label, ProgressBar, ProgressBarTrack } from "../primitives";
import { formatFileSize, formatSideSize } from "../format/fileSize";
import { FIND_SKIP } from "./find/findDom";
import { FIRST_ANNOUNCEMENT_DELAY_MS } from "./notices";
import type { StandInAction } from "./standInAction";
import { ViewerErrorMessage } from "./ViewerErrorMessage";

/** Whether the viewer has painted: a `Failure` that appears once it has is announced, and one it
 * opened with is not (`PaneFailure`'s `announce`). Provided by the viewer's content. */
export const ViewerPaintedContext = createContext(false);

/** Whether the viewer has not yet shown any content but the wait for it: a `Failure` that arrives
 * then comes with the dialog's focus move and waits `FIRST_ANNOUNCEMENT_DELAY_MS` like the status
 * regions; one after moving to another file is announced at once. Provided by the viewer's content. */
export const ViewerFirstContentContext = createContext(false);

/* What fills the body when the viewer has nothing to draw — a change whose two sides are the same,
 * a side that is not a file at all, a body it cannot present, a connection that is not there — is
 * the app's empty-state panel centred in the body, never a line left at the top of an empty dialog
 * ("The full panel" in `docs/product/empty-states.md`). */

/** The glyphs the stand-ins are drawn with, so the same kind of nothing looks the same wherever it
 * is reached from: nothing that can be shown at all, two sides that hold the same thing, and a side
 * that is not a file. */
const GLYPHS = { unshowable: FileQuestion, unchanged: Equal, symlink: Link, submodule: FolderGit2 } as const;

/** The messages of a side that is not a file, by kind. */
const NOT_FILE: Record<"symlink" | "submodule", { message: PlainMessageKey; description: PlainMessageKey }> = {
  symlink: { message: "viewer.change.symlink", description: "viewer.change.symlink.description" },
  submodule: { message: "viewer.change.submodule", description: "viewer.change.submodule.description" },
};

/** The messages of a lost connection, by what was being read: the line under the heading, and the
 * sentence the surface above speaks (and a notice would show) for the whole state. */
export const DISCONNECTED: Record<"file" | "change", { description: PlainMessageKey; sentence: PlainMessageKey }> = {
  file: { description: "viewer.disconnected.file.description", sentence: "viewer.disconnected.file.sentence" },
  change: { description: "viewer.disconnected.change.description", sentence: "viewer.disconnected.change.sentence" },
};

/** The stand-in for a change with nothing to draw: no patch, or a patch with no lines. */
export function Unreadable(): React.ReactElement {
  const t = useT();
  return <CenteredEmptyPanel icon={GLYPHS.unshowable} message={t("viewer.change.unreadable")} />;
}

/** The stand-in for a change whose two sides hold the same content, renamed or not. */
export function Unchanged({ kind }: { kind: "identical" | "renamedUnchanged" }): React.ReactElement {
  const t = useT();
  // A rename has no description: the rename is the whole state, and one would only repeat the title.
  return (
    <CenteredEmptyPanel
      icon={GLYPHS.unchanged}
      message={t(kind === "identical" ? "viewer.change.identical" : "viewer.change.renamedUnchanged")}
      description={kind === "identical" ? t("viewer.change.identical.description") : undefined}
    />
  );
}

/** The stand-in for a side that is not a file: a symbolic link, or a repository of its own. */
export function NotFile({ kind }: { kind: "symlink" | "submodule" }): React.ReactElement {
  const t = useT();
  return (
    <CenteredEmptyPanel
      icon={GLYPHS[kind]}
      message={t(NOT_FILE[kind].message)}
      description={t(NOT_FILE[kind].description)}
    />
  );
}

/**
 * Content shown only as what it is and its size: a body that cannot be displayed or is too large to, or a change
 * between two of them (`sizes` then holds both sides', `null` for a side with none). A single size that is `null` is one
 * the daemon did not say. `action` is what a live file offers outside the viewer (`StandInAction`): a button that opens
 * it in the default application, or Download and open, which while the copy is made gives way to its progress with a
 * Cancel; `name` is the file's, which the progress is named for.
 */
export function Unsupported({ message, sizes, action, name = "" }: { message: string; sizes: (number | null)[]; action?: StandInAction; name?: string }): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const download = action?.kind === "download" ? action.download : undefined;
  const progress = download?.progress;
  // Focus on the button that begins the copy goes to Cancel when the button gives way to it, and back to the button when
  // the copy ends (done, failed or cancelled) and Cancel goes; a copy begun from the header leaves focus where it is.
  const startRef = useRef<HTMLButtonElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);
  // Whether the press that began the copy was on the button, which hands focus to Cancel: the start is then announced
  // after that focus move, which would otherwise replace it. Set by the button's press, and put down once the copy has
  // ended, so that one begun from the header next is not taken for it.
  const [startedFromButton, setStartedFromButton] = useState(false);
  if (progress === undefined && startedFromButton) setStartedFromButton(false);
  const handoff = useFocusHandoff(() => (progress === undefined ? startRef : cancelRef).current?.focus());
  const description =
    sizes.length === 1 ? (
      sizes[0] === null ? undefined : formatFileSize(language, sizes[0])
    ) : (
      // Old to new runs left to right whatever the language, each size isolated in its own
      // direction, so a right-to-left language reorders neither the sizes nor the arrow.
      <span dir="ltr">
        <bdi>{formatSideSize(language, sizes[0])}</bdi> → <bdi>{formatSideSize(language, sizes[1])}</bdi>
      </span>
    );
  const label =
    action?.kind === "open"
      ? { label: t("viewer.openInDefaultApplication"), icon: SquareArrowOutUpRight, onPress: action.run }
      : download && progress === undefined
        ? { label: t("viewer.downloadAndOpen"), icon: Download, onPress: () => {
          setStartedFromButton(document.activeElement === startRef.current);
          download.start();
        } }
        : undefined;
  return (
    <div className="contents" onFocus={handoff.onFocus} onBlur={handoff.onBlur}>
      <CenteredEmptyPanel
        icon={GLYPHS.unshowable}
        message={message}
        description={description}
        progress={download && progress ? <DownloadProgress name={name} progress={progress} onCancel={download.cancel} cancelRef={cancelRef} afterFocusMove={startedFromButton} /> : undefined}
        action={label}
        actionRef={startRef}
      />
    </div>
  );
}

/** How far a copy of the file has come, in the stand-in's place of its button: a bar named for the file, which stays on
 * one line and fades where it is too long, how much has arrived of how much there is, which is never broken, and Cancel.
 * The start of the copy is announced once (`afterFocusMove`: the press that began it moved focus to Cancel); while the
 * size is not known the bar is indeterminate and described by the text saying so. */
function DownloadProgress({ name, progress, onCancel, cancelRef, afterFocusMove }: { name: string; progress: { received: number; total: number | null }; onCancel: () => void; cancelRef: React.Ref<HTMLButtonElement>; afterFocusMove: boolean }): React.ReactElement {
  const t = useT();
  const waitId = useId();
  const language = useCurrentLanguage();
  const { received, total } = progress;
  const amount = total === null ? undefined : t("viewer.download.progress", { received: formatFileSize(language, received), total: formatFileSize(language, total) });
  const label = t("viewer.download.label", { name: isolate(name) });
  return (
    <div className="flex w-72 max-w-full flex-col gap-3">
      <StatusAnnouncer text={label} firstWriteDelayMs={afterFocusMove ? AFTER_FOCUS_MOVE_DELAY_MS : undefined} />
      <ProgressBar value={received} maxValue={Math.max(total ?? 1, 1)} indeterminate={total === null} valueLabel={amount} describedBy={amount === undefined ? waitId : undefined}>
        <div className="flex items-baseline justify-between gap-3 text-sm">
          <Label className="min-w-0 flex-1 text-start">
            <FadeOverflow as="span" className="block min-w-0" titleWhenClipped={label}>
              {label}
            </FadeOverflow>
          </Label>
          <span id={amount === undefined ? waitId : undefined} className="shrink-0 whitespace-nowrap text-app-muted">{amount ?? t("viewer.download.sizeUnknown")}</span>
        </div>
        <ProgressBarTrack />
      </ProgressBar>
      <Button ref={cancelRef} size="sm" variant="secondary" className="self-center" onPress={onCancel}>
        {t("common.cancel")}
      </Button>
    </div>
  );
}

/**
 * Nothing read yet while the connection to the daemon is lost: a passing state, not a failure of
 * the file or change, so it is said in the same neutral stand-in as the rest and announced as a
 * status by the surface above, rather than as an error. Hidden from screen readers here so it is
 * not read twice. The subject is read again once the connection is back.
 */
export function Disconnected({ what }: { what: "file" | "change" }): React.ReactElement {
  const t = useT();
  return (
    <CenteredEmptyPanel
      icon={Unplug}
      message={t("viewer.disconnected")}
      description={t(DISCONNECTED[what].description)}
      silent
    />
  );
}

/**
 * Why the subject cannot be shown at all: the heaviest of the four, and the only one that replaces
 * the content with an error rather than standing in for it. It takes the empty-state shape in the
 * danger tone.
 */
export function Failure({ message, title, path }: { message: ErrorPresentation; title: string; path: string }): React.ReactElement {
  const painted = useContext(ViewerPaintedContext);
  const first = useContext(ViewerFirstContentContext);
  const delay = first ? FIRST_ANNOUNCEMENT_DELAY_MS : undefined;
  return <PaneFailure title={title} message={message} summary={<ViewerErrorMessage error={message} path={path} />} announce={painted} announceDelayMs={delay} />;
}

/** Visual only. The viewer's loading is spoken from its own status region, so this overlay, inserted
 * already holding its text, stays silent; only the renderer's pause while it draws a large file's
 * code goes unspoken. */
export function Loading(): React.ReactElement {
  const t = useT();
  return <LoadingState label={t("viewer.loading")} />;
}

/** `Loading` laid over the frame that holds the content, for it to be centred in the frame (it only
 * centres itself inside a flex parent). It goes over the content rather than beside it, so taking it
 * away does not lay out a large file a second time. The frame is `relative`. Find does not search it. */
export function LoadingOverlay(): React.ReactElement {
  return (
    <div {...FIND_SKIP} className="absolute inset-0 flex">
      <Loading />
    </div>
  );
}
