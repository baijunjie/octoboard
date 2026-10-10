import { Alert, Button, Chip } from "@heroui/react";
import { ChevronLeft, ChevronRight, FileQuestion, Unplug } from "lucide-react";
import React, { useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";

import { FadeOverflow } from "../components/FadeOverflow";
import { PathText } from "../components/PathText";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { TitledControl } from "../components/TitledControl";
import { Dialog, useRefocusIfLost } from "../dialogs/Dialog";
import { joinPhrases } from "../i18n/joinPhrases";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { PlainMessageKey, Translate } from "../i18n/catalog";
import { useOctoboardTheme } from "../theme";
import { displayWirePath, wireBaseName } from "../wirePath";
import { arrowNavigation } from "./arrowKeys";
import { diffPlan } from "./budgets";
import { CodeSurface, DiffSurface, Loading, Unreadable, useRendererScope } from "./CodeSurface";
import {
  changePresentation,
  changeStatus,
  hasTwoSides,
  type ViewerBody,
  type ViewerChange,
  type ViewerChangeSide,
  type ViewerContent,
  type ViewerSubject,
} from "./content";
import { ControlsSlot } from "./controlsSlot";
import { diffLayout, LayoutToggle, type DiffLayout } from "./diffLayout";
import { formatFileSize, formatSideSize } from "./format";
import { StatusChip } from "./StatusChip";
import type { StatusKey } from "./statusMarks";
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
  const [layoutSlot, setLayoutSlot] = useState<HTMLElement | null>(null);
  const { claimWrap, claimed: wrapClaimed } = useWrapClaims();
  const controls = useMemo(() => ({ layout: layoutSlot, claimWrap }), [layoutSlot, claimWrap]);
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

  // The header is two rows of fixed height, whatever the subject is: the title, with the tags of what
  // the change is before the name, and under it one row of path and description with the view
  // controls (the diff layout and word wrap choices) at its end. Every part that only some subjects
  // have (the tags, the controls, the size) sits in a row that is as tall without it, and each row
  // is one line cut by a fade, never wrapped, so moving between files cannot move the code below.
  // An untracked file is a change with an absent old side, which `changeStatus` calls added; the
  // list marks it untracked and says so in `subject.status`, which also holds while the content is
  // not read (yet). Without one, a change's sides give it, and a path in conflict has none to compare.
  const status: StatusKey | undefined =
    subject.status ?? (content.state === "change" ? changeStatus(content.change) : content.state === "conflict" ? "conflicted" : undefined);
  // The description's parts as the one string that is its tooltip when it is cut, in the order drawn.
  const renamedFrom = content.state === "change" ? renamedFromPath(content.change) : undefined;
  const description = joinPhrases(
    language,
    [
      subject.sourceText ?? (typeof subject.source === "string" ? subject.source : undefined),
      content.state === "file" ? formatFileSize(language, content.body.size) : undefined,
      // LRI…PDI is the plain-text form of the `dir="ltr"` the drawn path carries.
      renamedFrom === undefined ? undefined : t("viewer.change.renamedFrom", { path: `\u2066${displayWirePath(renamedFrom)}\u2069` }),
    ].filter((part): part is string => typeof part === "string"),
  );
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
          <span aria-hidden="true" className="flex min-w-0 items-center gap-2">
            {/* What the change is, coloured, then where it is from, neutral: both tags come before the name,
                whichever the subject has, and never shrink, so the name is what the fade cuts. */}
            {status && <StatusChip status={status}>{statusLabel}</StatusChip>}
            {subject.stage !== undefined && (
              <Chip size="sm" variant="soft">
                {subject.stage}
              </Chip>
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
      <div ref={headerRef} data-viewer-description="" className="flex h-8 min-w-0 shrink-0 items-center gap-3 text-xs text-muted">
        <div className="flex min-w-0 flex-1 items-center gap-3">
          <PathText path={displayWirePath(subject.path)} className="min-w-0 shrink" />
          <FadeOverflow as="span" className="flex min-w-0 shrink items-center gap-3" titleWhenClipped={description}>
            {subject.source && <span>{subject.source}</span>}
            {content.state === "file" && <span>{formatFileSize(language, content.body.size)}</span>}
            {renamedFrom !== undefined && <RenamedFrom path={renamedFrom} />}
          </FadeOverflow>
        </div>
        {/* Two fixed places, the layout choice before the wrap choice, so the Tab order is the
            order on screen whichever of them mounts first. The layout slot has no box of its own
            while empty, so it takes none of the gap. */}
        <div className="flex shrink-0 items-center gap-2">
          <div ref={setLayoutSlot} className="contents" />
          {wrapClaimed && <WrapToggleHost />}
        </div>
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

function RenamedFrom({ path }: { path: string }): React.ReactElement {
  return (
    <span>
      <Message id="viewer.change.renamedFrom" params={{ path: <span dir="ltr">{displayWirePath(path)}</span> }} />
    </span>
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
      return <BodyView resetKey={key} name={name} body={content.body} theme={theme} />;
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

/** One file body, whichever kind it is. */
function BodyView({
  resetKey,
  name,
  body,
  theme,
}: {
  resetKey: string;
  name: string;
  body: ViewerBody;
  theme: "light" | "dark";
}): React.ReactElement {
  const t = useT();
  switch (body.kind) {
    case "text":
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
    <div className="viewer-checkerboard flex min-h-0 flex-1 items-center justify-center overflow-hidden rounded-xl p-2">
      {/* The `error` event is the signal: an image's size is no test (an SVG with only a `viewBox`
          has none of its own) and `decode()` has wrongly rejected valid SVGs in older WebKit. */}
      <img src={body.url} alt={name} onError={() => setFailed(true)} className="max-h-full max-w-full object-contain" />
    </div>
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
      return <BodyView resetKey={resetKey} name={name} body={presentation.body} theme={theme} />;
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
