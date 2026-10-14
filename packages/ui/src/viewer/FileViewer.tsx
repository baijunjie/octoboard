import { Button, Chip } from "@heroui/react";
import { ChevronLeft, ChevronRight, Copy, File } from "lucide-react";
import React, { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import { FadeOverflow } from "../components/FadeOverflow";
import { PlainMarkedPath } from "../components/MarkedPath";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { TitledControl } from "../components/TitledControl";
import { ROW_ICON_BUTTON_CLASS, RowIconGroup } from "../components/RowControls";
import { useCopyPath } from "../components/useCopyPath";
import { Dialog, useRefocusIfLost } from "../dialogs/Dialog";
import { isolate } from "../i18n/isolate";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import { useOctoboardTheme } from "../theme";
import { displayWirePath, wireBaseName } from "../wirePath";
import { arrowNavigation } from "./arrowKeys";
import { useRendererScope } from "./codeRenderer";
import { CodeSurface, DiffSurface } from "./CodeSurface";
import {
  ChangeBodiesError,
  changePresentation,
  changeStatus,
  isMarkdownName,
  type ChangeBodies,
  type ViewerBody,
  type ViewerChange,
  type ViewerChangeSide,
  type ViewerContent,
  type ViewerSubject,
} from "./content";
import { ControlsSlot } from "./controlsSlot";
import { diffLayout, type DiffLayout } from "./diffLayout";
import { formatFileSize } from "./format";
import { FrameSurface } from "./FrameSurface";
import { MarkdownLocationProvider, type MarkdownLocation } from "./markdownLocation";
import { MarkdownSurface } from "./MarkdownSurface";
import { hasTwoSides } from "./patch";
import { StatusChip } from "./StatusChip";
import { STATUS_LABELS, type StatusKey } from "./statusMarks";
import {
  Disconnected,
  DISCONNECTED,
  Failure,
  FIRST_ANNOUNCEMENT_DELAY_MS,
  Loading,
  NotFile,
  Notice,
  Unchanged,
  Unreadable,
  Unsupported,
} from "./viewerStates";
import { useWrapClaims, WrapToggleHost } from "./wordWrap";

/** Moving to the subject before or after this one, from the header's buttons and from Left and Right;
 * a missing callback disables its button and its key. */
export interface ViewerNavigation {
  onPrevious?: () => void;
  onNext?: () => void;
}

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
}: {
  subject: ViewerSubject;
  onClose: () => void;
  navigation?: ViewerNavigation;
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

  // A navigation button that turns disabled while it has focus — the last subject reached with
  // Next, or the list changing under the viewer — loses focus to `<body>`, taking the dialog's
  // Escape and Tab containment with it. Focus moves to the other button when it still leads
  // somewhere, and to the dialog otherwise. Which button had focus is remembered from its own
  // focus events, since by the time this runs WebKit may already have moved focus off it.
  const previousRef = useRef<HTMLButtonElement>(null);
  const nextRef = useRef<HTMLButtonElement>(null);
  // On an element the viewer always renders, so the dialog is found even when the buttons are gone.
  const headerRef = useRef<HTMLDivElement>(null);
  const focusedNavigation = useRef<"previous" | "next">(undefined);
  const hasPrevious = Boolean(navigation?.onPrevious);
  const hasNext = Boolean(navigation?.onNext);
  useLayoutEffect(() => {
    const which = focusedNavigation.current;
    if (!which || (which === "previous" ? hasPrevious : hasNext)) return;
    const button = which === "previous" ? previousRef.current : nextRef.current;
    if (document.activeElement !== button && document.activeElement !== document.body) return;
    const other = which === "previous" ? (hasNext ? nextRef.current : null) : hasPrevious ? previousRef.current : null;
    (other ?? headerRef.current?.closest<HTMLElement>("[role=dialog]"))?.focus();
  }, [subject.key, hasPrevious, hasNext]);
  // The subject's content can be replaced under the same subject — read again, or the connection
  // lost — taking the code region that held focus with it, and the wrap choice goes once the last
  // claim on it is released, a commit after the content changed; the dialog takes focus back, or
  // its Escape and Tab would stop working.
  useRefocusIfLost(() => headerRef.current?.closest<HTMLElement>("[role=dialog]"), [content, wrapClaimed]);
  // The latest callbacks, for a listener added once.
  const latestNavigation = useRef(navigation);
  latestNavigation.current = navigation;
  useEffect(() => {
    const dialog = headerRef.current?.closest<HTMLElement>("[role=dialog]");
    if (!dialog) return;
    const onKeyDown = (event: KeyboardEvent) => {
      const direction = arrowNavigation(event, dialog);
      if (!direction) return;
      event.preventDefault();
      event.stopPropagation();
      (direction === "previous" ? latestNavigation.current?.onPrevious : latestNavigation.current?.onNext)?.();
    };
    dialog.addEventListener("keydown", onKeyDown);
    return () => dialog.removeEventListener("keydown", onKeyDown);
  }, []);

  const track = (which: "previous" | "next") => ({
    onFocus: () => (focusedNavigation.current = which),
    onBlur: (event: React.FocusEvent) => {
      // A blur to nowhere is the button turning disabled under focus; anything else is the user
      // moving on.
      if (event.relatedTarget !== null && focusedNavigation.current === which) focusedNavigation.current = undefined;
    },
  });

  const navigationControls = navigation ? (
    <RowIconGroup>
      <NavigationButton
        ref={previousRef}
        label={t("viewer.previous")}
        onPress={navigation.onPrevious}
        icon="previous"
        {...track("previous")}
      />
      <NavigationButton ref={nextRef} label={t("viewer.next")} onPress={navigation.onNext} icon="next" {...track("next")} />
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
  const tags = [statusLabel, subject.stage].filter((tag): tag is string => tag !== undefined);
  const titleName = tags.length > 0 ? `${name}${t("viewer.titleSeparator")}${tags.join(t("viewer.tagSeparator"))}` : name;
  const copyPath = useCopyPath();
  return (
    <Dialog
      size="viewer"
      headerLeading={navigationControls}
      title={
        <>
          {/* WebKit puts a space around every flex item when it computes an accessible name
              ("Modified , Staged : a.ts"), so the name is one string built here from the catalog and
              the drawn boxes are hidden from it. select-none keeps a copied heading from holding the
              text twice. */}
          <span className="sr-only select-none">{titleName}</span>
          <span aria-hidden="true" className="flex min-w-0 items-center gap-2.5">
            <FadeOverflow as="span" dir="ltr" className="min-w-0" titleWhenClipped={name}>
              {name}
            </FadeOverflow>
            {/* What the change is, coloured, then where it is from, neutral: both tags come after the name,
                whichever the subject has, and never shrink, so the name is what the fade cuts. The
                tags sit closer to each other than the group does to the name. */}
            {tags.length > 0 && (
              <span className="flex shrink-0 items-center gap-1">
                {status && <StatusChip status={status}>{statusLabel}</StatusChip>}
                {subject.stage !== undefined && (
                  <Chip size="sm" variant="soft">
                    {subject.stage}
                  </Chip>
                )}
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
              titleWhenClipped={subject.sourceText ?? (typeof subject.source === "string" ? subject.source : undefined)}
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
      <div ref={headerRef} data-viewer-description="" className="flow-root min-h-8 min-w-0 shrink-0 text-xs leading-8">
        {/* Copy path, then fixed places for the Markdown view choice, the layout choice and the
            wrap choice, so Tab order matches the screen whichever mounts first.
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
          {copyPath && <CopyPathButton onPress={() => copyPath(subject.path)} />}
          <div ref={setMarkdownViewSlot} className="contents" />
          <div ref={setLayoutSlot} className="contents" />
          {wrapClaimed && <WrapToggleHost />}
        </RowIconGroup>
        <DetailItem>
          <PlainMarkedPath path={displayWirePath(subject.path)} icon={File} />
        </DetailItem>
        {content.state === "file" && (
          <DetailItem>
            <Chip size="sm" variant="soft">
              {formatFileSize(language, content.body.size)}
            </Chip>
          </DetailItem>
        )}
        {renamedFrom !== undefined && <RenamedFrom path={renamedFrom} />}
      </div>
      <ControlsSlot value={controls}>
        <MarkdownLocationProvider value={{ path: subject.path, ...location }}>
          <ViewerContentView subject={subject} name={name} layout={layout} onLayoutChange={diffLayout.set} />
        </MarkdownLocationProvider>
      </ControlsSlot>
    </Dialog>
  );
}

function NavigationButton({
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
        size="sm"
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
function CopyPathButton({ onPress }: { onPress: () => void }): React.ReactElement {
  const t = useT();
  const label = t("common.copyPath");
  return (
    <TitledControl title={label}>
      <Button isIconOnly size="sm" variant="ghost" className={ROW_ICON_BUTTON_CLASS} aria-label={label} onPress={onPress} preventFocusOnPress>
        <Copy aria-hidden="true" className="size-4" />
      </Button>
    </TitledControl>
  );
}

/** For a rename within the project, the path it came from. */
function renamedFromPath(change: ViewerChange): string | undefined {
  return change.old.state === "present" && change.new.state === "present" && change.old.path !== change.new.path ? change.old.path : undefined;
}

/** One detail of the header, an item of its own that flows around the view controls and can take a
 * row of its own when it does not fit beside them. Its `inline-flex` is not free to change:
 * `RenamedFrom` puts a path into the middle of a sentence, which works only because the sentence's
 * text run becomes an anonymous flex item beside the path's own flex item, spaced by `gap-1.5`
 * rather than by the space in the message. In a non-flex display the path, a block-level flex box,
 * would break the line. */
function DetailItem({ children }: { children: React.ReactNode }): React.ReactElement {
  return <span className="me-4 inline-flex h-8 max-w-full items-center gap-1.5 whitespace-nowrap align-top text-app-muted">{children}</span>;
}

function RenamedFrom({ path }: { path: string }): React.ReactElement {
  return (
    <DetailItem>
      <Message id="viewer.change.renamedFrom" params={{ path: <PlainMarkedPath path={displayWirePath(path)} icon={File} /> }} />
    </DetailItem>
  );
}

/** Whether the content is still on its way: the file or change being read, or a conflict waiting
 * for its file. */
function isWaiting(content: ViewerContent): boolean {
  return content.state === "loading" || (content.state === "conflict" && !content.body && content.message === undefined);
}

function ViewerContentView(props: React.ComponentProps<typeof ViewerContentBody>): React.ReactElement {
  const t = useT();
  const { content } = props.subject;
  const announcement =
    isWaiting(content)
      ? t("viewer.loading")
      : content.state === "disconnected"
        ? t(DISCONNECTED[content.what].sentence)
        : undefined;
  // Outside the body, which swaps its whole tree as the content arrives, so the region is there
  // before its text is.
  return (
    <>
      <StatusAnnouncer text={announcement} firstWriteDelayMs={FIRST_ANNOUNCEMENT_DELAY_MS} />
      <ViewerContentBody {...props} />
    </>
  );
}

function ViewerContentBody({
  subject,
  name,
  layout,
  onLayoutChange,
}: {
  subject: ViewerSubject;
  name: string;
  layout: DiffLayout;
  onLayoutChange: (layout: DiffLayout) => void;
}): React.ReactElement {
  const t = useT();
  const { resolved: theme } = useOctoboardTheme();
  const { content, key } = subject;
  switch (content.state) {
    case "loading":
      return <Loading />;
    case "error":
      return <Failure title={t("viewer.error")} message={content.message} />;
    case "disconnected":
      return <Disconnected what={content.what} />;
    case "file":
      return <BodyView resetKey={key} name={name} body={content.body} theme={theme} asDocument />;
    case "conflict":
      return (
        <>
          <Notice tone="warned">{content.conflict}</Notice>
          {content.body ? (
            <BodyView resetKey={key} name={name} body={content.body} theme={theme} />
          ) : content.message !== undefined ? (
            <Failure title={t("viewer.error")} message={content.message} />
          ) : (
            <Loading />
          )}
        </>
      );
    case "change":
      return (
        <ChangeView
          resetKey={key}
          name={name}
          change={content.change}
          layout={layout}
          onLayoutChange={onLayoutChange}
          theme={theme}
        />
      );
  }
}

/** One file body, whichever kind it is. A Markdown file's text is offered as a document when
 * `asDocument` says so: a whole file shown alone, not a side of a comparison or a conflict body, whose
 * markers are what it is looked at for. */
function BodyView({
  resetKey,
  name,
  body,
  theme,
  asDocument = false,
}: {
  resetKey: string;
  name: string;
  body: ViewerBody;
  theme: "light" | "dark";
  asDocument?: boolean;
}): React.ReactElement {
  const t = useT();
  switch (body.kind) {
    case "text":
      if (asDocument && isMarkdownName(name)) return <MarkdownSurface resetKey={resetKey} name={name} text={body.text} theme={theme} />;
      return <CodeSurface resetKey={resetKey} name={name} text={body.text} theme={theme} />;
    case "image":
      return <ImageView key={resetKey} name={name} body={body} theme={theme} />;
    case "binary":
      return <Unsupported message={t("viewer.binary")} sizes={[body.size]} />;
  }
}

/** An image scaled down to fit, never up. One that cannot be decoded falls back to its own text
 * when it has some (an SVG), and to the unsupported presentation otherwise. Keyed by its subject by
 * the caller, so a failure is never carried over to the next image. */
function ImageView({
  name,
  body,
  theme,
}: {
  name: string;
  body: Extract<ViewerBody, { kind: "image" }>;
  theme: "light" | "dark";
}): React.ReactElement {
  const t = useT();
  const [failed, setFailed] = useState(false);
  if (failed) {
    return body.text !== undefined ? (
      <>
        {/* The image fails only once it has been fetched and decoded, long after the dialog was
            drawn, so the fall back to its text is spoken as well as shown. */}
        <Notice tone="failed" speak={t("viewer.image.asText")} />
        <CodeSurface resetKey={body.url} name={name} text={body.text} theme={theme} />
      </>
    ) : (
      <Unsupported message={t("viewer.image.failed")} sizes={[body.size]} />
    );
  }
  return (
    <FrameSurface>
      <div className="viewer-checkerboard flex min-h-0 flex-1 items-center justify-center overflow-hidden rounded-xl p-2">
        {/* The `error` event is the signal: an image's size is no test (an SVG with only a `viewBox`
            has none of its own) and `decode()` has wrongly rejected valid SVGs in older WebKit. */}
        <img src={body.url} alt={name} onError={() => setFailed(true)} className="max-h-full max-w-full object-contain" />
      </div>
    </FrameSurface>
  );
}

function ChangeView({
  resetKey,
  name,
  change,
  layout,
  onLayoutChange,
  theme,
}: {
  resetKey: string;
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
      return (
        <>
          <Notice tone="warned">
            <Message
              id={presentation.hidden === "old" ? "viewer.change.outsideOld" : "viewer.change.outsideNew"}
              params={{ path: <span dir="ltr">{displayWirePath(presentation.repositoryPath)}</span> }}
            />
          </Notice>
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
      return presentation.message !== undefined ? <Failure title={t("viewer.error")} message={presentation.message} /> : <Unreadable />;
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
 * reason above it when there is one to give.
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
  const [read, setRead] = useState<{ key: string; text?: string; why?: "failed" | "unavailable" | "changed"; message?: string }>();
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
        const message = error instanceof Error ? error.message : String(error);
        setRead({ key: resetKey, why: reason === "superseded" ? "unavailable" : reason, message });
      },
    );
    return () => {
      current = false;
    };
  }, [resetKey, loadBodies]);
  if (!loadBodies) return <Unchanged kind={kind} />;
  // The body is shown all the same from here on, so the state is a notice over it rather than a
  // stand-in panel: it is said in one sentence, not as a heading with a line under it.
  const said = t(kind === "identical" ? "viewer.change.identical.sentence" : "viewer.change.renamedUnchanged.sentence");
  // Keyed on the subject alone, so a re-read of the same one keeps what is on screen until it answers.
  const done = read?.key === resetKey ? read : undefined;
  const reason =
    done === undefined || done.text !== undefined
      ? undefined
      : done.why === "failed"
        ? { tone: "failed" as const, text: t("viewer.change.contentFailed", { reason: isolate(done.message ?? "") }) }
        : { tone: "explained" as const, text: t(done.why === "changed" ? "viewer.change.contentChanged" : "viewer.change.contentUnavailable") };
  return (
    <>
      {/* Mounted for as long as the change is, so the read's own progress and its outcome are both
          spoken: every one of them arrives after the dialog was drawn. */}
      <StatusAnnouncer text={done === undefined ? t("viewer.loading") : (reason?.text ?? said)} firstWriteDelayMs={FIRST_ANNOUNCEMENT_DELAY_MS} />
      {done === undefined ? (
        <Loading />
      ) : done.text !== undefined ? (
        <>
          <Notice announced>{said}</Notice>
          {/* Nothing reads a text body's size; the daemon sends none for a change's side either. */}
          <BodyView resetKey={resetKey} name={name} body={{ kind: "text", text: done.text, size: done.text.length }} theme={theme} asDocument />
        </>
      ) : (
        <>
          {reason && (
            <Notice tone={reason.tone} announced>
              {reason.text}
            </Notice>
          )}
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
    <section aria-label={label} className="flex min-h-0 min-w-0 flex-col gap-1">
      <h3 className="text-xs font-medium text-app-muted">{label}</h3>
      {side.state === "present" && side.body ? (
        <BodyView resetKey={resetKey} name={name} body={side.body} theme={theme} />
      ) : (
        side.state !== "out_of_scope" && <SideNote side={side} />
      )}
    </section>
  );
}

/** A side with nothing to show: absent, or present without a body that was read. */
function SideNote({ side }: { side: Exclude<ViewerChangeSide, { state: "out_of_scope" }> }): React.ReactElement {
  const t = useT();
  const message = side.state === "absent" ? t("viewer.change.absent") : t("viewer.change.noContent");
  // The dashed border, not a centred glyph, is what marks an empty column's extent, so this one
  // stand-in keeps a shape of its own; its sentence reads in the foreground like every other.
  return (
    <div className="flex min-h-0 flex-1 items-center justify-center rounded-xl border border-dashed border-separator p-4 text-center text-sm text-foreground">
      {message}
    </div>
  );
}
