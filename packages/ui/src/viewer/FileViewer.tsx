import { Alert, Button, Chip } from "@heroui/react";
import { ChevronLeft, ChevronRight, FileQuestion } from "lucide-react";
import React, { useEffect, useLayoutEffect, useRef, useState } from "react";

import { PathText } from "../components/PathText";
import { TitledControl } from "../components/TitledControl";
import { Dialog } from "../dialogs/Dialog";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { PlainMessageKey } from "../i18n/catalog";
import { useOctoboardTheme } from "../theme";
import { displayWirePath, wireBaseName } from "../wirePath";
import { arrowNavigation } from "./arrowKeys";
import { CodeSurface, DiffSurface, Loading, useRendererScope } from "./CodeSurface";
import {
  changePresentation,
  changeStatus,
  type ChangeStatus,
  type ViewerBody,
  type ViewerChange,
  type ViewerChangeSide,
  type ViewerSubject,
} from "./content";
import { formatFileSize, formatSideSize } from "./format";

const STATUS_LABELS: Record<ChangeStatus, PlainMessageKey> = {
  added: "viewer.change.added",
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
  const [layout, setLayout] = useState<"unified" | "split">("unified");
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

  return (
    <Dialog
      size="viewer"
      title={<span dir="ltr">{name}</span>}
      onClose={onClose}
      footer={footer}
      resetKey={subject.key}
    >
      <div ref={headerRef} className="flex min-w-0 shrink-0 flex-wrap items-center gap-x-3 gap-y-2 text-xs text-muted">
        <PathText path={displayWirePath(subject.path)} className="min-w-0 max-w-full" />
        {subject.source && <span>{subject.source}</span>}
        {content.state === "file" && <span>{formatFileSize(language, content.body.size)}</span>}
        {content.state === "change" && <ChangeSummary change={content.change} />}
      </div>
      <ViewerContentView subject={subject} name={name} layout={layout} onLayoutChange={setLayout} />
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

/** The change's status, and for a rename within the project the path it came from. */
function ChangeSummary({ change }: { change: ViewerChange }): React.ReactElement {
  const t = useT();
  const status = changeStatus(change);
  const from = change.old.state === "present" && change.new.state === "present" && change.old.path !== change.new.path ? change.old.path : undefined;
  return (
    <>
      <Chip size="sm" variant="soft">
        {t(STATUS_LABELS[status])}
      </Chip>
      {from !== undefined && (
        <span>
          <Message id="viewer.change.renamedFrom" params={{ path: <span dir="ltr">{displayWirePath(from)}</span> }} />
        </span>
      )}
    </>
  );
}

function ViewerContentView({
  subject,
  name,
  layout,
  onLayoutChange,
}: {
  subject: ViewerSubject;
  name: string;
  layout: "unified" | "split";
  onLayoutChange: (layout: "unified" | "split") => void;
}): React.ReactElement {
  const t = useT();
  const { resolved: theme } = useOctoboardTheme();
  const { content, key } = subject;
  switch (content.state) {
    case "loading":
      return <Loading />;
    case "error":
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
                {content.message}
              </span>
            </Alert.Description>
          </Alert.Content>
        </Alert>
      );
    case "file":
      return <BodyView resetKey={key} name={name} body={content.body} theme={theme} />;
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
        <p className="shrink-0 px-1 pb-2 text-xs text-muted">{t("viewer.image.asText")}</p>
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
  layout: "unified" | "split";
  onLayoutChange: (layout: "unified" | "split") => void;
  theme: "light" | "dark";
}): React.ReactElement {
  const t = useT();
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
    case "text":
      return (
        <DiffSurface
          resetKey={resetKey}
          name={name}
          patch={presentation.patch}
          layout={layout}
          onLayoutChange={onLayoutChange}
          theme={theme}
        />
      );
    case "image":
      return (
        <div className="@container flex min-h-0 flex-1">
          <div className="grid min-h-0 flex-1 grid-rows-2 gap-3 @2xl:grid-cols-2 @2xl:grid-rows-1">
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
      return <p className="text-sm text-muted">{t("viewer.change.unreadable")}</p>;
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
