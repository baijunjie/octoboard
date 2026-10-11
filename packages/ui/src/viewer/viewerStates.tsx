// Everything the viewer shows in place of, or above, the content itself: the notice over content it
// is still drawing, the stand-in that fills the body when there is nothing to draw, the error block
// for a subject that cannot be opened at all, and the spinner while any of it is on its way. They
// are all here so the four stay one family.
import { Alert, Spinner } from "@heroui/react";
import { Equal, FileQuestion, FolderGit2, Link, Unplug, type LucideIcon } from "lucide-react";
import React from "react";

import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { useCurrentLanguage, useT } from "../i18n/react";
import { formatFileSize, formatSideSize } from "./format";

/** How long the viewer's status regions wait before their first text. The dialog takes focus as it
 * opens and VoiceOver announces that, replacing a status written within about 300 ms of it; a
 * write 1.5 s later is spoken after it (verified). Moving to another file in an open viewer has
 * no such focus move and is announced at once. */
export const FIRST_ANNOUNCEMENT_DELAY_MS = 1500;

/**
 * What kind of thing a notice is saying, which is what tells them apart on screen.
 *
 * Pick the tone by what the sentence tells the user, which is all the user has to go on:
 *
 * - `explained` — it states how the content below is drawn and why, for what this file or change
 *   is: a size budget, a change read at an older version, lines left collapsed because the whole
 *   file is not to be had.
 * - `warned` — it says something about the content that has to be known before reading it: a file
 *   still in conflict, a side of the change that lies outside the project.
 * - `failed` — it reports that something went wrong, so the view that was asked for is not the one
 *   on screen: a read that errored, a diff that could not be drawn, an image that would not decode.
 *
 * A sentence worded as one and meaning another is a fault in the sentence, not a reason to tone it
 * against its wording: change the message, and the tone follows.
 */
export type NoticeTone = "explained" | "warned" | "failed";

/** Each tone is a HeroUI alert status, which draws the glyph that goes with it — the circled `i`,
 * the warning triangle, the circled `!` — so the three differ in shape and not only in fill
 * (`docs/ui-accessibility.md`, "Not by colour alone"). The status draws no fill of its own: HeroUI's
 * alert takes the plain surface, which equals a dialog's own, so each tone names the soft fill to go
 * with it. `--default-soft` reads as a block only because a dialog carries `.control-fills`, which
 * makes it the opaque control fill; on HeroUI's own half-tint it would be about 1.1:1 and no block
 * at all. */
const TONES: Record<NoticeTone, { status: "default" | "warning" | "danger"; fill: string }> = {
  explained: { status: "default", fill: "bg-default-soft" },
  warned: { status: "warning", fill: "bg-warning-soft" },
  failed: { status: "danger", fill: "bg-danger-soft" },
};

/**
 * What is said above content the viewer is still showing: why it is shown this way, what to know
 * about it first, or what part of it could not be shown. Every one of them is this one block, so
 * they stack as one strip and none reads as a stray line crowding the view controls beside it.
 *
 * A notice that can appear after the surface has been drawn would otherwise never reach a reader,
 * so it is spoken as well as shown: pass it as `speak`, which is the sentence and, unless
 * `children` says otherwise, the visible text too — one string, so the two cannot drift apart. Pass
 * `announced` instead where the surface already has a region speaking that sentence. Either way the
 * block itself is hidden from assistive technology, so nothing is read twice. A notice that is
 * there from the first paint passes neither.
 */
export function Notice({
  tone = "explained",
  speak,
  announced,
  children = speak,
}: {
  tone?: NoticeTone;
  speak?: string;
  announced?: boolean;
  children?: React.ReactNode;
}): React.ReactElement {
  const { status, fill } = TONES[tone];
  const hidden = speak !== undefined || announced === true;
  return (
    <>
      {/* Tighter than HeroUI's own padding: a notice sits between the header and the code, and the
          stock block takes a row of the code frame away from every change that carries one. */}
      <Alert aria-hidden={hidden || undefined} status={status} className={`shrink-0 gap-3 px-3 py-2 shadow-none ${fill}`}>
        <Alert.Indicator />
        <Alert.Content>
          <Alert.Description className="text-foreground">{children}</Alert.Description>
        </Alert.Content>
      </Alert>
      {speak !== undefined && <StatusAnnouncer text={speak} firstWriteDelayMs={FIRST_ANNOUNCEMENT_DELAY_MS} />}
    </>
  );
}

/**
 * What fills the body when the viewer has nothing to draw: a change whose two sides are the same, a
 * side that is not a file at all, a body it cannot present, a connection that is not there. One
 * centred glyph and sentence, never a line left at the top of an empty dialog.
 *
 * `silent` is for a state spoken from a region elsewhere, which hides this copy of it so it is not
 * read twice. `children` is for whatever the state adds under the sentence, such as a size.
 */
export function NothingToShow({
  icon: Icon,
  message,
  silent,
  children,
}: {
  icon: LucideIcon;
  message: string;
  silent?: boolean;
  children?: React.ReactNode;
}): React.ReactElement {
  return (
    <div
      aria-hidden={silent}
      className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 text-center text-sm text-muted"
    >
      <Icon aria-hidden="true" className="size-8" />
      <p className="text-foreground">{message}</p>
      {children}
    </div>
  );
}

/** The glyphs the stand-ins are drawn with, so the same kind of nothing looks the same wherever it
 * is reached from: nothing that can be shown at all, two sides that hold the same thing, and a side
 * that is not a file. */
const GLYPHS = { unshowable: FileQuestion, unchanged: Equal, symlink: Link, submodule: FolderGit2 } as const;

/** The stand-in for a change with nothing to draw: no patch, or a patch with no lines. */
export function Unreadable(): React.ReactElement {
  const t = useT();
  return <NothingToShow icon={GLYPHS.unshowable} message={t("viewer.change.unreadable")} />;
}

/** The stand-in for a change whose two sides hold the same content, renamed or not. */
export function Unchanged({ kind }: { kind: "identical" | "renamedUnchanged" }): React.ReactElement {
  const t = useT();
  return <NothingToShow icon={GLYPHS.unchanged} message={t(kind === "identical" ? "viewer.change.identical" : "viewer.change.renamedUnchanged")} />;
}

/** The stand-in for a side that is not a file: a symbolic link, or a repository of its own. */
export function NotFile({ kind }: { kind: "symlink" | "submodule" }): React.ReactElement {
  const t = useT();
  return <NothingToShow icon={GLYPHS[kind]} message={t(kind === "symlink" ? "viewer.change.symlink" : "viewer.change.submodule")} />;
}

/**
 * Content shown only as what it is and its size: a body that cannot be displayed, or a change
 * between two of them (`sizes` then holds both sides', `null` for a side with none).
 */
export function Unsupported({ message, sizes }: { message: string; sizes: (number | null)[] }): React.ReactElement {
  const language = useCurrentLanguage();
  return (
    <NothingToShow icon={GLYPHS.unshowable} message={message}>
      {sizes.length === 1 ? (
        <p>{formatFileSize(language, sizes[0] ?? 0)}</p>
      ) : (
        // Old to new runs left to right whatever the language, each size isolated in its own
        // direction, so a right-to-left language reorders neither the sizes nor the arrow.
        <p dir="ltr">
          <bdi>{formatSideSize(language, sizes[0])}</bdi> → <bdi>{formatSideSize(language, sizes[1])}</bdi>
        </p>
      )}
    </NothingToShow>
  );
}

/**
 * Nothing read yet while the connection to the daemon is lost: a passing state, not a failure of
 * the file or change, so it is said in the same neutral stand-in as the rest and announced as a
 * status by the surface above, rather than as an error. Hidden from screen readers here so it is
 * not read twice. The subject is read again once the connection is back.
 */
export function Disconnected({ message }: { message: string }): React.ReactElement {
  return <NothingToShow icon={Unplug} message={message} silent />;
}

/**
 * Why the subject cannot be shown at all: the heaviest of the four, and the only one that replaces
 * the content with an error rather than standing in for it. It takes the danger fill the failure
 * notice takes, so the two read as one severity and neither is louder than the other.
 */
export function Failure({ message, title }: { message: string; title: string }): React.ReactElement {
  return (
    <Alert status="danger" className="shrink-0 bg-danger-soft shadow-none">
      <Alert.Indicator />
      <Alert.Content>
        {/* HeroUI's alert carries no role, so a failure that arrives after the user moved on
            would never be announced. The alert below carries the title too, for screen readers,
            so the visible title is hidden from them and the title is read once. */}
        <Alert.Title aria-hidden="true">{title}</Alert.Title>
        <Alert.Description>
          <span role="alert">
            <span className="sr-only">{title} </span>
            {message}
          </span>
        </Alert.Description>
      </Alert.Content>
    </Alert>
  );
}

/** Visual only. The viewer's loading is spoken from its own status region, so this overlay, inserted
 * already holding its text, stays silent; only the renderer's pause while it draws a large file's
 * code goes unspoken. */
export function Loading(): React.ReactElement {
  const t = useT();
  return (
    <div aria-hidden="true" className="flex min-h-0 flex-1 items-center justify-center gap-2 text-sm text-muted">
      <Spinner size="sm" aria-hidden="true" />
      {t("viewer.loading")}
    </div>
  );
}

/** `Loading` laid over the frame that holds the content, for it to be centred in the frame (it only
 * centres itself inside a flex parent). It goes over the content rather than beside it, so taking it
 * away does not lay out a large file a second time. The frame is `relative`. */
export function LoadingOverlay(): React.ReactElement {
  return (
    <div className="absolute inset-0 flex">
      <Loading />
    </div>
  );
}
