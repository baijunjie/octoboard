import React, { useEffect, useState } from "react";

import { InlineReference } from "../i18n/InlineReference";
import { StatusAnnouncer } from "../components/announce/StatusAnnouncer";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { MessageParams } from "../protocol";
import { displayWirePath } from "../format/wirePath";
import { BodyView } from "./BodyView";
import { DiffSurface } from "./CodeSurface";
import {
  bodiesFailure,
  ChangeBodiesError,
  changePresentation,
  type ChangeBodies,
  type ViewerChange,
  type ViewerChangeSide,
} from "./content";
import type { DiffLayout } from "./diffLayout";
import { hasTwoSides } from "./patch";
import { FileNotice, FIRST_ANNOUNCEMENT_DELAY_MS, NoticeSlotBoxes, NoticeSlotsProvider } from "./notices";
import { Failure, Loading, NotFile, Unchanged, Unreadable, Unsupported } from "./viewerStates";
import { BodiesFailureReason } from "./ViewerErrorMessage";

export function ChangeView({
  resetKey,
  path,
  name,
  change,
  layout,
  onLayoutChange,
  theme,
}: {
  resetKey: string;
  path: string;
  name: string;
  change: ViewerChange;
  layout: DiffLayout;
  onLayoutChange: (layout: DiffLayout) => void;
  theme: "light" | "dark";
}): React.ReactElement {
  const t = useT();
  const presentation = changePresentation(change);
  switch (presentation.kind) {
    case "restricted": {
      const shown = presentation.shown;
      const outside = presentation.hidden === "old" ? "viewer.change.outsideOld" : "viewer.change.outsideNew";
      return (
        <>
          <FileNotice
            tone="warned"
            sentence={t(outside, { path: isolate(displayWirePath(presentation.repositoryPath)) })}
            line={<Message id={outside} params={{ path: <InlineReference monospace wrap>{displayWirePath(presentation.repositoryPath)}</InlineReference> }} />}
          />
          {shown.state === "present" && shown.body ? (
            // The side is shown alone because the other one cannot be reached, not as a side of a
            // comparison, so a Markdown one is offered as a document like any whole file.
            <BodyView resetKey={resetKey} name={name} body={shown.body} theme={theme} asDocument />
          ) : (
            shown.state !== "out_of_scope" && <SideNote side={shown} />
          )}
        </>
      );
    }
    case "single":
      // No note that there is nothing to compare it with: this relies on the caller marking an untracked
      // file's change `untracked` (`subject.status`), whose chip then says so.
      // A whole file body, so a Markdown one is offered as a document like a file read for itself.
      return <BodyView resetKey={resetKey} name={name} body={presentation.body} theme={theme} asDocument />;
    case "text": {
      const twoSides = hasTwoSides(presentation.patch);
      return (
        <DiffSurface
          resetKey={resetKey}
          name={name}
          patch={presentation.patch}
          layout={twoSides ? layout : "unified"}
          onLayoutChange={twoSides ? onLayoutChange : undefined}
          theme={theme}
          loadBodies={presentation.expandable ? change.loadBodies : undefined}
          lines={{ old: change.old.state === "present" ? change.old.lines : undefined, new: change.new.state === "present" ? change.new.lines : undefined }}
        />
      );
    }
    case "identical":
      return <UnchangedView resetKey={resetKey} name={name} kind="identical" loadBodies={change.loadBodies} theme={theme} />;
    case "renamedUnchanged":
      return <UnchangedView resetKey={resetKey} name={name} kind="renamedUnchanged" loadBodies={change.loadBodies} theme={theme} />;
    case "notFile":
      return <NotFile kind={presentation.sideKind} />;
    case "image":
      return (
        <div className="@container flex min-h-0 flex-1">
          <div className="grid min-h-0 flex-1 grid-rows-2 gap-3 @2xl:grid-cols-2 @2xl:grid-rows-1">
            {/* An image that cannot be decoded can fall back to its text, on either side; the wrap
                choice in the header serves whichever sides did. */}
            <ImageSide label={t("viewer.change.before")} side={change.old} name={name} resetKey={`${resetKey}:old`} theme={theme} />
            <ImageSide label={t("viewer.change.after")} side={change.new} name={name} resetKey={`${resetKey}:new`} theme={theme} />
          </div>
        </div>
      );
    case "binary": {
      const size = (side: ViewerChangeSide) => (side.state === "present" && side.body ? side.body.size : null);
      return <Unsupported message={t("viewer.change.binary")} sizes={[size(change.old), size(change.new)]} />;
    }
    case "unreadable":
      return presentation.message !== undefined ? <Failure title={t("viewer.error")} message={presentation.message} path={path} /> : <Unreadable />;
  }
}

/**
 * A change with no lines to draw but content worth showing: the two sides hold the same text, under
 * one name or two. A patch with no hunk carries no body of its own (see "Changes and comparisons"
 * in `apps/daemon/PROTOCOL.md`), so the content is read the way a diff's collapsed lines are, once
 * as the change opens. Without that read the viewer would say the file is unchanged and show none
 * of it, while a rename across the project's boundary — which cannot be diffed either — shows the
 * whole file; the two are the same kind of change and now look it.
 *
 * Where the bodies cannot be had after all, it falls back to the stand-in it used to be, with the
 * reason above it when there is one to give. A read that failed is a warning, not an error: the
 * stand-in is true (it comes from Git's empty patch) and only the preview of the content is lost.
 */
function UnchangedView({
  resetKey,
  name,
  kind,
  loadBodies,
  theme,
}: {
  resetKey: string;
  name: string;
  kind: "identical" | "renamedUnchanged";
  loadBodies?: () => Promise<ChangeBodies>;
  theme: "light" | "dark";
}): React.ReactElement {
  const t = useT();
  // What the read answered for this subject: its text, or why there is none. A read given up for a
  // newer one is left as `unavailable`, like any other content that is not to be had, rather than
  // as a spinner nothing would resolve.
  const [read, setRead] = useState<{ key: string; text?: string; why?: "failed" | "unavailable" | "changed"; message?: string; detail?: string; failure?: { code: string; params: MessageParams } }>();
  useEffect(() => {
    if (!loadBodies) return;
    let current = true;
    void loadBodies().then(
      (bodies) => {
        if (!current) return;
        // The sentence above the content asserts that the two sides hold the same thing, so the
        // bodies have to bear that out. The daemon refuses the one case known to break it — a side
        // read from the worktree at a path with a clean filter, whose patch is numbered from the
        // filtered content and not from the bytes on disk — but the assertion is this view's to
        // make, so it is checked here rather than assumed.
        setRead(bodies.old === bodies.new ? { key: resetKey, text: bodies.new } : { key: resetKey, why: "unavailable" });
      },
      (error: unknown) => {
        if (!current) return;
        const reason = error instanceof ChangeBodiesError ? error.reason : "failed";
        setRead({ key: resetKey, why: reason === "superseded" ? "unavailable" : reason, ...bodiesFailure(error) });
      },
    );
    return () => {
      current = false;
    };
  }, [resetKey, loadBodies]);
  if (!loadBodies) return <Unchanged kind={kind} />;
  // The body is shown all the same from here on, so the state is a mark about it rather than a
  // stand-in panel: it is said in one sentence, not as a heading with a line under it.
  const said = t(kind === "identical" ? "viewer.change.identical.sentence" : "viewer.change.renamedUnchanged.sentence");
  // Keyed on the subject alone, so a re-read of the same one keeps what is on screen until it answers.
  const done = read?.key === resetKey ? read : undefined;
  // The stand-in is true, so a failed read loses only the preview: a warning, in the application's
  // words. A failure with no words of its own gives no cause (the generic "something unexpected"
  // sentence would say nothing); the raw text stays in the mark's popover alone. Where the cause sets
  // names as chips, the popover shows it so (`line`).
  const cause = done?.message;
  const reason =
    done === undefined || done.text !== undefined
      ? undefined
      : done.why === "failed"
        ? {
          tone: "warned" as const,
          text: cause === undefined ? t("viewer.change.contentNotShown") : t("viewer.change.contentNotShownBecause", { reason: isolate(cause) }),
          line: cause !== undefined ? <Message id="viewer.change.contentNotShownBecause" params={{ reason: <BodiesFailureReason message={cause} failure={done.failure} /> }} /> : undefined,
          detail: done.detail,
        }
        : { tone: "explained" as const, text: t(done.why === "changed" ? "viewer.change.contentChanged" : "viewer.change.contentUnavailable"), line: undefined, detail: undefined };
  return (
    <>
      {/* Mounted for as long as the change is, so the read's own progress and its outcome are both
          spoken: every one of them arrives after the dialog was drawn. */}
      <StatusAnnouncer text={done === undefined ? t("viewer.loading") : (reason?.text ?? said)} firstWriteDelayMs={FIRST_ANNOUNCEMENT_DELAY_MS} clearAfterRead />
      {done === undefined ? (
        <Loading />
      ) : done.text !== undefined ? (
        <>
          <FileNotice sentence={said} />
          {/* Nothing reads a text body's size; the daemon sends none for a change's side either. */}
          <BodyView resetKey={resetKey} name={name} body={{ kind: "text", text: done.text, size: done.text.length }} theme={theme} asDocument />
        </>
      ) : (
        <>
          {reason && <FileNotice tone={reason.tone} sentence={reason.text} line={reason.line} detail={reason.detail} />}
          <Unchanged kind={kind} />
        </>
      )}
    </>
  );
}

/** One side of an image change, labelled; an absent side says so instead of showing nothing. */
function ImageSide({
  label,
  side,
  name,
  resetKey,
  theme,
}: {
  label: string;
  side: ViewerChangeSide;
  name: string;
  resetKey: string;
  theme: "light" | "dark";
}): React.ReactElement {
  return (
    <NoticeSlotsProvider>
      <section aria-label={label} className="flex min-h-0 min-w-0 flex-col">
        {/* The notices of this side's content stand beside its heading, not in the file's header. The row
            keeps the heading's line height, so a side with marks starts its content where the other side does. */}
        <div className="flex min-w-0 items-center gap-1.5 px-3 py-2">
          <h3 className="text-xs font-medium text-app-muted">{label}</h3>
          <NoticeSlotBoxes />
        </div>
        {side.state === "present" && side.body ? (
          <BodyView resetKey={resetKey} name={name} body={side.body} theme={theme} />
        ) : (
          side.state !== "out_of_scope" && <SideNote side={side} />
        )}
      </section>
    </NoticeSlotsProvider>
  );
}

/** A side with nothing to show: absent, or present without a body that was read. */
function SideNote({ side }: { side: Exclude<ViewerChangeSide, { state: "out_of_scope" }> }): React.ReactElement {
  const t = useT();
  const message = side.state === "absent" ? t("viewer.change.absent") : t("viewer.change.noContent");
  return (
    <div className="viewer-checkerboard flex min-h-0 flex-1 items-center justify-center p-4 text-center text-sm text-foreground">
      {message}
    </div>
  );
}
