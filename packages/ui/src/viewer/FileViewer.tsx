import { Alert, Button, Chip } from "@heroui/react";
import { ChevronLeft, ChevronRight, File, FileQuestion, Unplug } from "lucide-react";
import React, { useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";

import { FadeOverflow } from "../components/FadeOverflow";
import { PlainMarkedPath } from "../components/MarkedPath";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { TitledControl } from "../components/TitledControl";
import { Dialog, useRefocusIfLost } from "../dialogs/Dialog";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { PlainMessageKey, Translate } from "../i18n/catalog";
import { useOctoboardTheme } from "../theme";
import { displayWirePath, wireBaseName } from "../wirePath";
import { arrowNavigation } from "./arrowKeys";
import { diffPlan } from "./budgets";
import { useRendererScope } from "./codeRenderer";
import { CodeSurface, DiffSurface } from "./CodeSurface";
import {
  changePresentation,
  changeStatus,
  hasTwoSides,
  isMarkdownName,
  type ViewerBody,
  type ViewerChange,
  type ViewerChangeSide,
  type ViewerContent,
  type ViewerSubject,
} from "./content";
import { ControlsSlot } from "./controlsSlot";
import { diffLayout, LayoutToggle, type DiffLayout } from "./diffLayout";
import { formatFileSize, formatSideSize } from "./format";
import { FrameSurface } from "./FrameSurface";
import { MarkdownSurface } from "./MarkdownSurface";
import { StatusChip } from "./StatusChip";
import type { StatusKey } from "./statusMarks";
import { Loading, Unreadable } from "./viewerStates";
import { useWrapClaims, WrapToggleHost } from "./wordWrap";

const STATUS_LABELS: Record<StatusKey, PlainMessageKey> = {
  added: "viewer.change.added",
  untracked: "viewer.change.untracked",
  conflicted: "viewer.change.conflicted",
  deleted: "viewer.change.deleted",
  renamed: "viewer.change.renamed",
  typeChanged: "viewer.change.typeChanged",
  modified: "viewer.change.modified",
};

/** Moving to the subject before or after this one, from the footer's buttons and from Left and Right;
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
}: {
  subject: ViewerSubject;
  onClose: () => void;
  navigation?: ViewerNavigation;
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

  const footer = navigation ? (
    <>
      <NavigationButton
        ref={previousRef}
        label={t("viewer.previous")}
        onPress={navigation.onPrevious}
        icon="previous"
        {...track("previous")}
      />
      <NavigationButton ref={nextRef} label={t("viewer.next")} onPress={navigation.onNext} icon="next" {...track("next")} />
    </>
  ) : null;

  // The header is the title, with the tags of what the change is before the name, and under it the
  // details (path, source, size, rename origin) beside the view controls (the Markdown view, diff
  // layout and word wrap choices), which float at the row's end. The details are separate items laid
  // out like words around that float (`DetailItem`): they share the row with the controls to use the
  // width beside them, and one too long for the space left moves down to a row of its own, where it
  // has the whole width, rather than being squeezed against the others. Each item is one line cut by
  // a fade, so only a detail that does not fit even alone is clipped.
  // The block is the row beside the controls plus a row for each further line the dropped items
  // need (short items that all miss the space beside the controls share one): two rows when a long
  // comparison drops, three when a rename's long origin drops too, each taking 32 px of the code
  // area from the dialog. That is accepted, because the milestone asks that a long detail take a
  // row of its own rather than be squeezed, and capping it would squeeze one. It also gives up what
  // the header held before, each row one line that is never wrapped, so that moving between files
  // could not move the code below; the code now moves when the rows needed differ from one file to
  // the next.
  // An untracked file is a change with an absent old side, which `changeStatus` calls added; the
  // list marks it untracked and says so in `subject.status`, which also holds while the content is
  // not read (yet). Without one, a change's sides give it, and a path in conflict has none to compare.
  const status: StatusKey | undefined =
    subject.status ?? (content.state === "change" ? changeStatus(content.change) : content.state === "conflict" ? "conflicted" : undefined);
  const renamedFrom = content.state === "change" ? renamedFromPath(content.change) : undefined;
  const statusLabel = status ? t(STATUS_LABELS[status]) : undefined;
  const tags = [statusLabel, subject.stage].filter((tag): tag is string => tag !== undefined);
  const titleName = tags.length > 0 ? `${tags.join(t("viewer.tagSeparator"))}${t("viewer.titleSeparator")}${name}` : name;
  return (
    <Dialog
      size="viewer"
      title={
        <>
          {/* WebKit puts a space around every flex item when it computes an accessible name
              ("Modified , Staged : a.ts"), so the name is one string built here from the catalog and
              the drawn boxes are hidden from it. select-none keeps a copied heading from holding the
              text twice. */}
          <span className="sr-only select-none">{titleName}</span>
          <span aria-hidden="true" className="flex min-w-0 items-center gap-2.5">
            {/* What the change is, coloured, then where it is from, neutral: both tags come before the name,
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
            <FadeOverflow as="span" dir="ltr" className="min-w-0" titleWhenClipped={name}>
              {name}
            </FadeOverflow>
          </span>
        </>
      }
      onClose={onClose}
      footer={footer}
      resetKey={subject.key}
    >
      <div ref={headerRef} data-viewer-description="" className="flow-root min-h-8 min-w-0 shrink-0 text-xs leading-8">
        {/* Fixed places, the Markdown view choice, then the layout choice, then the wrap choice, so
            the Tab order is the order on screen whichever of them mounts first. The slots have no
            box of their own while empty, so they take none of the gap. The group floats to the end of
            the row, ahead of the details in the source so the line next to it is the one it shortens
            (a float shortens only the lines that follow it). That puts the three control groups before
            the file's path for a screen reader. That cost was weighed and accepted: the details are
            independent, separately labelled facts and the path is not needed to use the controls. The
            dialog's body text is muted, which a toggle's unselected label would inherit (under 4.5:1
            on a hovered `.control-fills` fill), so the controls take the foreground colour and each
            detail item the muted one. */}
        <div className="float-end flex h-8 items-center gap-2 text-foreground">
          <div ref={setMarkdownViewSlot} className="contents" />
          <div ref={setLayoutSlot} className="contents" />
          {wrapClaimed && <WrapToggleHost />}
        </div>
        <DetailItem>
          <PlainMarkedPath path={displayWirePath(subject.path)} icon={File} />
        </DetailItem>
        {subject.source && (
          <DetailItem>
            <FadeOverflow
              as="span"
              className="min-w-0"
              titleWhenClipped={subject.sourceText ?? (typeof subject.source === "string" ? subject.source : undefined)}
            >
              {subject.source}
            </FadeOverflow>
          </DetailItem>
        )}
        {content.state === "file" && <DetailItem>{formatFileSize(language, content.body.size)}</DetailItem>}
        {renamedFrom !== undefined && <RenamedFrom path={renamedFrom} />}
      </div>
      <ControlsSlot value={controls}>
        <ViewerContentView subject={subject} name={name} layout={layout} onLayoutChange={diffLayout.set} />
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
        variant="secondary"
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
  return <span className="me-4 inline-flex h-8 max-w-full items-center gap-1.5 whitespace-nowrap align-top text-muted">{children}</span>;
}

function RenamedFrom({ path }: { path: string }): React.ReactElement {
  return (
    <DetailItem>
      <Message id="viewer.change.renamedFrom" params={{ path: <PlainMarkedPath path={displayWirePath(path)} icon={File} /> }} />
    </DetailItem>
  );
}

/** How long the viewer's status region waits before its first text. The dialog takes focus as it
 * opens and VoiceOver announces that, replacing a status written within about 300 ms of it; a
 * write 1.5 s later is spoken after it (verified). Moving to another file in an open viewer has
 * no such focus move and is announced at once. */
const FIRST_ANNOUNCEMENT_DELAY_MS = 1500;

/** Whether the content is still on its way: the file or change being read, or a conflict waiting
 * for its file. */
function isWaiting(content: ViewerContent): boolean {
  return content.state === "loading" || (content.state === "conflict" && !content.body && content.message === undefined);
}

function ViewerContentView(props: React.ComponentProps<typeof ViewerContentBody>): React.ReactElement {
  const t = useT();
  const { content } = props.subject;
  const announcement = isWaiting(content) ? t("viewer.loading") : content.state === "disconnected" ? disconnectedMessage(t, content.what) : undefined;
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
      return <Failure message={content.message} />;
    case "disconnected":
      return <Disconnected message={disconnectedMessage(t, content.what)} />;
    case "file":
      return <BodyView resetKey={key} name={name} body={content.body} theme={theme} asDocument />;
    case "conflict":
      return (
        <>
          <Alert status="warning" className="shrink-0 bg-warning/10 shadow-none">
            <Alert.Content>
              <Alert.Description>{content.conflict}</Alert.Description>
            </Alert.Content>
          </Alert>
          {content.body ? (
            <BodyView resetKey={key} name={name} body={content.body} theme={theme} />
          ) : content.message !== undefined ? (
            <Failure message={content.message} />
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

function disconnectedMessage(t: Translate, what: "file" | "change"): string {
  return t(what === "file" ? "viewer.disconnected.file" : "viewer.disconnected.change");
}

/**
 * Nothing read yet while the connection to the daemon is lost: a passing state, not a failure of
 * the file or change, so it is said in neutral text, announced as a status (by `ViewerContentView`,
 * whose region stays mounted) rather than as an error, and hidden from screen readers here so it is
 * not read twice. The subject is read again once the connection is back.
 */
function Disconnected({ message }: { message: string }): React.ReactElement {
  return (
    <div aria-hidden="true" className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 text-center text-sm text-muted">
      <Unplug aria-hidden="true" className="size-8" />
      <p>{message}</p>
    </div>
  );
}

/** Why the subject cannot be shown. */
function Failure({ message }: { message: string }): React.ReactElement {
  const t = useT();
  return (
    <Alert status="danger" className="shrink-0">
      <Alert.Content>
        {/* HeroUI's alert carries no role, so a failure that arrives after the user moved on
            would never be announced. The alert below carries the title too, for screen readers,
            so the visible title is hidden from them and the title is read once. */}
        <Alert.Title aria-hidden="true">{t("viewer.error")}</Alert.Title>
        <Alert.Description>
          <span role="alert">
            <span className="sr-only">{t("viewer.error")} </span>
            {message}
          </span>
        </Alert.Description>
      </Alert.Content>
    </Alert>
  );
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
        <p className="shrink-0 text-xs text-muted">{t("viewer.image.asText")}</p>
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

/** Content shown only as what it is and its size: a body that cannot be displayed, or a change
 * between two of them (`sizes` then holds both sides', `null` for a side with none). */
function Unsupported({ message, sizes }: { message: string; sizes: (number | null)[] }): React.ReactElement {
  const language = useCurrentLanguage();
  return (
    <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 text-center text-sm text-muted">
      <FileQuestion aria-hidden="true" className="size-8" />
      <p className="text-foreground">{message}</p>
      {sizes.length === 1 ? (
        <p>{formatFileSize(language, sizes[0] ?? 0)}</p>
      ) : (
        // Old to new runs left to right whatever the language, each size isolated in its own
        // direction, so a right-to-left language reorders neither the sizes nor the arrow.
        <p dir="ltr">
          <bdi>{formatSideSize(language, sizes[0])}</bdi> → <bdi>{formatSideSize(language, sizes[1])}</bdi>
        </p>
      )}
    </div>
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
  const sectionsId = useId();
  const presentation = changePresentation(change);
  switch (presentation.kind) {
    case "restricted": {
      const shown = presentation.shown;
      return (
        <>
          <Alert status="warning" className="shrink-0 bg-warning/10 shadow-none">
            <Alert.Content>
              <Alert.Description>
                <Message
                  id={presentation.hidden === "old" ? "viewer.change.outsideOld" : "viewer.change.outsideNew"}
                  params={{ path: <span dir="ltr">{displayWirePath(presentation.repositoryPath)}</span> }}
                />
              </Alert.Description>
            </Alert.Content>
          </Alert>
          {shown.state === "present" && shown.body ? (
            <BodyView resetKey={resetKey} name={name} body={shown.body} theme={theme} />
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
          loadBodies={change.loadBodies}
        />
      );
    }
    case "sections":
      // Each section drawn on its own, so a type change's removal and addition are never merged
      // into one diff; with two, they are its old and its new side.
      // One layout choice serves every section.
      return (
        <>
          {/* Only when some section is drawn as a diff with two sides: a plain patch has no layout,
              and a one-sided diff reads the same in both. */}
          {presentation.sections.some((section) => diffPlan(section) === "render" && hasTwoSides(section)) && (
            <LayoutToggle layout={layout} onLayoutChange={onLayoutChange} />
          )}
          <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto">
            {presentation.sections.map((section, i) => {
              const heading = presentation.sections.length === 2 ? `${sectionsId}-${i}` : undefined;
              return (
                <section key={i} aria-labelledby={heading} className="flex min-h-48 shrink-0 flex-col gap-1">
                  {heading && (
                    <h3 id={heading} className="text-xs font-medium text-muted">
                      {t(i === 0 ? "viewer.change.before" : "viewer.change.after")}
                    </h3>
                  )}
                  <DiffSurface
                    resetKey={`${resetKey}:${i}`}
                    name={name}
                    patch={section}
                    layout={hasTwoSides(section) ? layout : "unified"}
                    theme={theme}
                  />
                </section>
              );
            })}
          </div>
        </>
      );
    case "identical":
      return <p className="text-sm text-muted">{t("viewer.change.identical")}</p>;
    case "notFile":
      return (
        <p className="text-sm text-muted">{t(presentation.sideKind === "symlink" ? "viewer.change.symlink" : "viewer.change.submodule")}</p>
      );
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
      return presentation.message !== undefined ? <Failure message={presentation.message} /> : <Unreadable />;
  }
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
      <h3 className="text-xs font-medium text-muted">{label}</h3>
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
  return (
    <div className="flex min-h-0 flex-1 items-center justify-center rounded-xl border border-dashed border-separator p-4 text-center text-sm text-muted">
      {message}
    </div>
  );
}
