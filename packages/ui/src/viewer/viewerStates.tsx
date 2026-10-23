// Everything the viewer shows in place of, or above, the content itself: the notice over content it
// is still drawing, the stand-in that fills the body when there is nothing to draw, the error block
// for a subject that cannot be opened at all, and the spinner while any of it is on its way. They
// are all here so the four stay one family.
import { Alert, Spinner } from "@heroui/react";
import { Equal, FileQuestion, FolderGit2, Link, Unplug } from "lucide-react";
import React, { createContext, useContext } from "react";

import { CenteredEmptyPanel } from "../components/EmptyPanel";
import { FailureAlert } from "../components/FailureAlert";
import { PaneFailure } from "../components/PaneFailure";
import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../components/StatusAnnouncer";
import { TintedAlert } from "../components/TintedAlert";
import type { ErrorPresentation } from "../errorPresentation";
import type { PlainMessageKey } from "../i18n/catalog";
import { textDirection } from "../i18n/languages";
import { useCurrentLanguage, useT } from "../i18n/react";
import { formatFileSize, formatSideSize } from "./format";
import { ViewerErrorMessage } from "./ViewerErrorMessage";

/** How long the viewer's status regions wait before their first text: the dialog takes focus as it
 * opens. Moving to another file in an open viewer has no such focus move and is announced at once. */
export const FIRST_ANNOUNCEMENT_DELAY_MS = AFTER_FOCUS_MOVE_DELAY_MS;

/** Whether the viewer has painted: a `Failure` that appears once it has is announced, and one it
 * opened with is not (`ErrorAlert`'s `announce`). Provided by the viewer's content. */
export const ViewerPaintedContext = createContext(false);

/** Whether the viewer has not yet shown any content but the wait for it: a `Failure` that arrives
 * then comes with the dialog's focus move and waits `FIRST_ANNOUNCEMENT_DELAY_MS` like the status
 * regions; one after moving to another file is announced at once. Provided by the viewer's content. */
export const ViewerFirstContentContext = createContext(false);

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
 * alert takes the plain surface, which equals a dialog's own, so each tone uses its matching soft
 * fill. `--default-soft` reads as a block only because a dialog carries `.control-fills`, which
 * makes it the opaque control fill; on HeroUI's own half-tint it would be about 1.1:1 and no block
 * at all. */
const TONES: Record<NoticeTone, "default" | "warning" | "danger"> = {
  explained: "default",
  warned: "warning",
  failed: "danger",
};

/**
 * What is said above content the viewer is still showing: why it is shown this way, what to know
 * about it first, or what part of it could not be shown. Above a reading surface they stack;
 * inside a Markdown document they retain the same stock Alert parts.
 *
 * A notice that can appear after the surface has been drawn would otherwise never reach a reader,
 * so it is spoken as well as shown: pass it as `speak`, which is the sentence and, unless
 * `children` says otherwise, the visible text too — one string, so the two cannot drift apart. Pass
 * `announced` instead where the surface already has a region speaking that sentence. Either way the
 * block itself is hidden from assistive technology, so nothing is read twice. A failed notice is
 * spoken the same way, as a status and not an alert: it is a rendering fallback, with the content
 * still shown. A notice that is there from the first paint passes neither.
 */
export function Notice({
  tone = "explained",
  appearance = "strip",
  speak,
  announced,
  children = speak,
}: {
  tone?: NoticeTone;
  appearance?: "strip" | "document";
  speak?: string;
  announced?: boolean;
  children?: React.ReactNode;
}): React.ReactElement {
  const status = TONES[tone];
  const direction = textDirection(useCurrentLanguage());
  const inDocument = appearance === "document";
  const hidden = speak !== undefined || announced === true;
  return (
    <>
      <div className="shrink-0">
        <TintedAlert
          aria-hidden={hidden || undefined}
          dir={inDocument ? direction : undefined}
          status={status}
        >
          <Alert.Indicator />
          <Alert.Content>
            <Alert.Description>{children}</Alert.Description>
          </Alert.Content>
        </TintedAlert>
      </div>
      {speak !== undefined && <StatusAnnouncer text={speak} firstWriteDelayMs={FIRST_ANNOUNCEMENT_DELAY_MS} />}
    </>
  );
}

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
 * Content shown only as what it is and its size: a body that cannot be displayed, or a change
 * between two of them (`sizes` then holds both sides', `null` for a side with none).
 */
export function Unsupported({ message, sizes }: { message: string; sizes: (number | null)[] }): React.ReactElement {
  const language = useCurrentLanguage();
  return (
    <CenteredEmptyPanel
      icon={GLYPHS.unshowable}
      message={message}
      description={
        sizes.length === 1 ? (
          formatFileSize(language, sizes[0] ?? 0)
        ) : (
          // Old to new runs left to right whatever the language, each size isolated in its own
          // direction, so a right-to-left language reorders neither the sizes nor the arrow.
          <span dir="ltr">
            <bdi>{formatSideSize(language, sizes[0])}</bdi> → <bdi>{formatSideSize(language, sizes[1])}</bdi>
          </span>
        )
      }
    />
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
 * the content with an error rather than standing in for it. With nothing else beside it, it takes
 * the empty-state shape in the danger tone; `among` is for one that sits among other content (a
 * file in conflict shows its notice above), which stays a compact Alert.
 */
export function Failure({ message, title, path, among = false }: { message: ErrorPresentation; title: string; path: string; among?: boolean }): React.ReactElement {
  const painted = useContext(ViewerPaintedContext);
  const first = useContext(ViewerFirstContentContext);
  const delay = first ? FIRST_ANNOUNCEMENT_DELAY_MS : undefined;
  if (among) return <FailureAlert title={title} announce={painted} announceDelayMs={delay} message={message} summary={<ViewerErrorMessage error={message} path={path} />} />;
  return <PaneFailure title={title} message={message} summary={<ViewerErrorMessage error={message} path={path} />} announce={painted} announceDelayMs={delay} />;
}

/** Visual only. The viewer's loading is spoken from its own status region, so this overlay, inserted
 * already holding its text, stays silent; only the renderer's pause while it draws a large file's
 * code goes unspoken. */
export function Loading(): React.ReactElement {
  const t = useT();
  return (
    <div aria-hidden="true" className="flex min-h-0 flex-1 items-center justify-center gap-2 text-sm text-app-muted">
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
