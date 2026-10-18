import { AlertDialog, Button, Modal } from "@heroui/react";
import React, { useEffect, useId, useRef, useState } from "react";

import { DaemonErrorMessage, DaemonErrorTitle } from "../components/DaemonErrorMessage";
import { ErrorAlert } from "../components/ErrorAlert";
import { takeMenuFocusToRestore } from "../components/ActionMenu";
import { FadedScroller, useFadeRegion } from "../components/scrollFade";
import { TitledControl } from "../components/TitledControl";
import { useLatest } from "../components/useLatest";
import { errorPresentation, hasShellTimeoutDetails, type ErrorPresentation } from "../errorPresentation";
import { useT } from "../i18n/react";
import { prefersReducedMotion } from "../reducedMotion";
import { dialogAround } from "./dialogElement";
import { useEscapeWhileTooltipOpen } from "./useEscapeWhileTooltipOpen";
import { useHeightMotion } from "./useHeightMotion";
import { useToastClearance } from "./useToastClearance";

/** The inline error and busy state every dialog that talks to the daemon needs. The error is the
 * request's own failure, shown at the foot of the dialog; what is wrong with a single field goes to
 * that field's own `errorMessage` instead (see `useSubmitValidation`). `run` clears the
 * error, marks the dialog busy for the duration of `action`, and shows a rejection as the error
 * instead of letting it escape, so the dialog stays open for another try. A dialog reused for
 * successive subjects passes a `resetKey` naming the current one: when it changes the error and
 * busy state are cleared, and an action still running for the previous subject no longer reports
 * into them. */
export function useDialogAction(resetKey?: string): {
  error: ErrorPresentation | undefined;
  setError: (message: string | undefined) => void;
  busy: boolean;
  run: (action: () => Promise<void>) => Promise<void>;
} {
  const [error, setError] = useState<ErrorPresentation>();
  const [busy, setBusy] = useState(false);
  const [seenKey, setSeenKey] = useState(resetKey);
  const currentKey = useLatest(resetKey);
  if (seenKey !== resetKey) {
    setSeenKey(resetKey);
    setError(undefined);
    setBusy(false);
  }

  const run = async (action: () => Promise<void>) => {
    const startedFor = resetKey;
    const stale = () => currentKey.current !== startedFor;
    setBusy(true);
    setError(undefined);
    try {
      await action();
    } catch (err) {
      if (!stale()) setError(errorPresentation(err as Error));
    } finally {
      if (!stale()) setBusy(false);
    }
  };

  return { error, setError, busy, run };
}

/** Puts keyboard focus back on the element `target` returns when `deps` change and focus has fallen to `<body>`,
 * which is where it goes when the focused control is unmounted by the change. Left alone, a dialog's
 * Escape handling and Tab containment stop working, since they hang off focus being inside it. */
export function useRefocusIfLost(target: () => HTMLElement | null | undefined, deps: React.DependencyList): void {
  useEffect(() => {
    if (document.activeElement === document.body) target()?.focus();
  }, deps);
}

/** Holds back what a dialog's own checks found until the user has tried to submit, so a field is
 * not marked before it has been filled in. From then on the message follows what is typed, and
 * correcting the field takes it away. */
export function useSubmitValidation(): {
  /** `message` once a submit has found something wanting, nothing before that. */
  shown: <T extends string>(message: T | undefined) => T | undefined;
  /** Records the attempt and answers whether the dialog may go ahead with it. */
  attempt: (...messages: (string | undefined)[]) => boolean;
} {
  const [attempted, setAttempted] = useState(false);
  return {
    shown: (message) => (attempted ? message : undefined),
    attempt: (...messages) => {
      setAttempted(true);
      return messages.every((message) => message === undefined);
    },
  };
}

/** A dialog's inline failure line, for the failure of the request itself; a field's own validation
 * belongs under that field. A message the dialog words itself with markup (a file name as code) is
 * passed as an element. It sits at the foot of the body, which in a long form is out of view,
 * so it is scrolled into view when it appears or changes (smoothly, unless the system asks for
 * reduced motion); `nearest` leaves the body where it is when the line is already showing. */
export function DialogError({ message }: { message: ErrorPresentation | React.ReactElement | undefined }): React.ReactElement | null {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!message) return;
    let cancelled = false;
    let frame = requestAnimationFrame(() => {
      // The line's arrival changes the dialog's height, which `useHeightMotion` starts easing once
      // the browser has laid that out, a frame on. Scrolling waits for the dialog to have settled:
      // until then its body is held at the old height, and the line is not where it will be.
      frame = requestAnimationFrame(() => {
        const animations = dialogAround(ref)?.getAnimations?.() ?? [];
        void Promise.allSettled(animations.map((animation) => animation.finished)).then(() => {
          if (!cancelled) ref.current?.scrollIntoView?.({ block: "nearest", behavior: prefersReducedMotion() ? "auto" : "smooth" });
        });
      });
    });
    return () => {
      cancelled = true;
      cancelAnimationFrame(frame);
    };
  }, [message]);
  return message ? (
    <div ref={ref} className="shrink-0">
      {React.isValidElement(message) ? (
        <ErrorAlert>{message}</ErrorAlert>
      ) : (
        <ErrorAlert title={typeof message !== "string" && hasShellTimeoutDetails(message) ? <DaemonErrorTitle /> : undefined}>
          <DaemonErrorMessage error={message} showTitle={false} />
        </ErrorAlert>
      )}
    </div>
  ) : null;
}

/** The children HeroUI's modal parts accept. Their typings resolve `ReactNode` against the React 18
 * types that react-aria pulls in, which `React.ReactNode` from this package's React 19 does not
 * satisfy. */
type BodyChildren = React.ComponentProps<typeof Modal.Footer>["children"];

const NOT_TYPED_INTO = new Set(["checkbox", "radio", "button", "submit", "image", "reset", "range", "color", "file"]);

/** Whether `target` takes typed text, where a held key's repeats are what the user means. */
function isTextEntry(target: EventTarget): boolean {
  if (target instanceof HTMLTextAreaElement) return true;
  if (target instanceof HTMLElement && target.isContentEditable) return true;
  return target instanceof HTMLInputElement && !NOT_TYPED_INTO.has(target.type);
}

/** A control that holds a choice rather than acting: a closed select, a tab, a radio, a switch. */
const CHOICE_CONTROL = '[aria-haspopup="listbox"]:not([aria-expanded="true"]), [role="tab"], [role="radio"], [role="switch"]';

/**
 * Enter on a control that holds a choice submits the dialog, as it already does from a text field or a
 * checkbox: having made a choice, the user presses Enter to go ahead, not to open the picker again.
 * The control keeps Space and the arrow keys. A button that acts (Cancel, a disclosure) keeps Enter, and
 * so does whatever is open in a popover, which is outside the form's own elements. While the submit
 * button is disabled, or pending (react-aria then makes it a plain button), the key falls back to the
 * control.
 */
function submitsFromChoice(e: React.KeyboardEvent<HTMLFormElement>): boolean {
  if (e.key !== "Enter" || e.repeat) return false;
  const form = e.currentTarget;
  if (!(e.target instanceof Element) || !form.contains(e.target) || !e.target.matches(CHOICE_CONTROL)) return false;
  const submit = form.querySelector<HTMLButtonElement>('button[type="submit"]');
  if (!submit || submit.disabled) return false;
  e.preventDefault();
  e.stopPropagation();
  form.requestSubmit(submit);
  return true;
}

/** How many `Dialog`s are mounted, so a nested one can tell it is not the last to close. */
let openDialogs = 0;

/**
 * The frame every dialog shares. Escape and a click outside both call `onClose` (for an `alert`
 * dialog too, which HeroUI's `AlertDialog` would otherwise make explicit-action only); the dialog is
 * mounted only while it is open, so `isOpen` is constant. With `onSubmit` the header, body and footer sit in
 * a `<form>`, so Enter in a text field submits it — the dialog decides what "submit" means. The
 * footer is Cancel plus a submit button labelled `submitLabel`, unless the dialog brings its own
 * `footer`. While `busy`, the submit button is marked pending rather than disabled: a disabled
 * attribute would blur the button the user has just pressed and take the dialog's Escape and Tab
 * with it for as long as the request runs, where pending blocks press and hover, keeps the button
 * focusable and announces itself. Cancel is disabled instead, which costs nothing because focus
 * cannot be on it at the moment the request starts. `submitDisabled` disables the submit button
 * alone, before any action, Cancel stays pressable, and since it is the form's only submit control
 * this also suppresses implicit submission from Enter in a text field. A form dialog puts
 * `autoFocus` on its first text field, so Enter submits at once; one with no text field passes
 * `autoFocusSubmit` to open on the submit button instead (a dialog with its own `footer` puts
 * `autoFocus` on its own button).
 */
export function Dialog({
  title,
  headerLeading,
  subtitle,
  onClose,
  children,
  footer,
  submitLabel,
  busy,
  submitDisabled,
  autoFocusSubmit,
  onSubmit,
  size = "md",
  alert,
  resetKey,
}: {
  /** Text, or an element for a title that needs markup of its own (a file name kept left to right). */
  title: string | React.ReactElement;
  /** Controls beside the title, outside the heading's accessible name. */
  headerLeading?: React.ReactNode;
  /** A line under the title that belongs to it (where a compared change is from): in the header, so
   * it reads after the title and is never clipped by the body's scroller. It brings its own height. */
  subtitle?: React.ReactElement;
  onClose: () => void;
  children: BodyChildren;
  /** `wide` is `lg` widened for content laid out across it: a list of rows that each carry a path,
   * or groups of fields side by side; `viewer` is a frame for content rather than a form: it fills
   * the window inside a margin, up to a width that still reads as a dialog over the app. */
  size?: "sm" | "md" | "lg" | "wide" | "viewer";
  /** A confirmation that interrupts the user: HeroUI's `AlertDialog`, announced as an alert dialog. */
  alert?: boolean;
  /** Names what a dialog reused across subjects currently asks about; when it changes and focus
   * was lost with the old content, it is put back on the dialog. */
  resetKey?: string;
} & (
  | {
    /** `null` for a dialog without a footer. */
    footer: BodyChildren | null;
    submitLabel?: never;
    busy?: never;
    submitDisabled?: never;
    autoFocusSubmit?: never;
    onSubmit?: () => void;
  }
  | { footer?: never; submitLabel: string; busy?: boolean; submitDisabled?: boolean; autoFocusSubmit?: boolean; onSubmit: () => void }
)): React.ReactElement {
  const t = useT();
  // Closing returns focus to the element focused when the dialog opened, which for a dialog a menu
  // item opened is the menu's trigger. The menu knows better (the terminal, for a pointer press),
  // so that wins. Whichever runs first, the menu's element ends up focused: react-aria's own
  // restore is delayed a frame and acts only while focus is on `<body>`, and one that runs earlier
  // is overridden by this move. A dialog opened from inside another one (the directory picker)
  // leaves it for the outer dialog, which is the one the menu opened.
  useEffect(() => {
    openDialogs += 1;
    return () => {
      openDialogs -= 1;
      if (openDialogs > 0) return;
      const element = takeMenuFocusToRestore();
      if (element) setTimeout(() => element.isConnected && element.focus(), 0);
    };
  }, []);

  const markerRef = useRef<HTMLSpanElement>(null);
  useRefocusIfLost(() => dialogAround(markerRef), [resetKey]);
  useToastClearance(markerRef, footer !== null);
  useHeightMotion(markerRef);
  useEscapeWhileTooltipOpen(() => dialogAround(markerRef), onClose);

  const Frame = alert ? AlertDialog : Modal;
  // An alert dialog is described by its whole body, so a screen reader announces what is asked
  // together with the title; react-aria only wires up a `slot="description"` Text, which this body
  // is not.
  const bodyId = useId();
  // The body's scroll container is reachable from the keyboard while it scrolls: a dialog whose
  // controls are all in the footer (a long message, a page of text) has nothing in the body to
  // take focus, and the arrow keys and Page Down would otherwise move nothing. It is named by the
  // dialog's own title, which react-aria has already tied to the dialog.
  const [titleId, setTitleId] = useState<string>();
  useEffect(() => setTitleId(dialogAround(markerRef)?.getAttribute("aria-labelledby") ?? undefined), []);
  const region = useFadeRegion({ labelledBy: titleId });

  const body = (
    <>
      {/* The body is not the scroller itself: the `FadedScroller` inside it is, so that every
          dialog whose content outgrows the window fades at the edge it can still be scrolled
          towards, with no caller building a scroll container of its own. The fade is a mask that
          shows only while there is more to scroll and takes no pointer events, and it is a hint
          alone: the scroller is an ordinary overflow container, so the keyboard, the wheel and
          focus moving into view scroll it as they would any other. The padding is the room a focus
          ring needs inside the clip. */}
      {/* The ring of the focusable scroller is drawn on its parent: the fade is a mask on the scroller
            and clips by its border box, so a ring on the scroller itself, which sits outside that box, is
            cut off. The ring is the project's usual one (2px `--app-focus`), from the scroller's own state. */}
      <Frame.Body
        id={bodyId}
        className={`flex flex-col overflow-y-visible rounded-lg ${region.focused ? "ring-2 ring-app-focus" : ""}`}
      >
        <FadedScroller region={region} className="flex min-h-0 flex-1 flex-col gap-4 p-1">
          {children}
        </FadedScroller>
      </Frame.Body>
      {footer !== null && (
        <Frame.Footer className="flex-wrap">
          {footer ?? (
            <>
              <Button type="button" variant="secondary" onPress={onClose} isDisabled={busy}>
                {t("common.cancel")}
              </Button>
              <Button type="submit" isPending={busy} isDisabled={submitDisabled} autoFocus={autoFocusSubmit}>
                {submitLabel}
              </Button>
            </>
          )}
        </Frame.Footer>
      )}
    </>
  );

  // Keep the modal parts together: HeroUI spaces them through adjacent-sibling selectors.
  const content = (
    <>
      {/* HeroUI places the close button over the header's end without reserving room for it, so
          the heading is padded clear of it; a long title (a session's name, say) has to wrap
          anywhere, since a name with no place to break would run out of the dialog. */}
      <Frame.Header className={headerLeading ? "grid grid-cols-[auto_minmax(0,1fr)] items-center gap-x-2 gap-y-1 pe-6" : subtitle ? "gap-1 pe-6" : "pe-6"}>
        {headerLeading}
        <Frame.Heading className="min-w-0 [overflow-wrap:anywhere]">{title as BodyChildren}</Frame.Heading>
        {subtitle}
      </Frame.Header>
      {body}
    </>
  );

  return (
    <Frame.Backdrop isOpen isDismissable isKeyboardDismissDisabled={false} onOpenChange={(open) => !open && onClose()}>
      {/* HeroUI pads the container 40px from `sm` up, which below the `docked` breakpoint leaves too
          little of a narrow window for code; the viewer asks for a 16px margin there and the 40px
          one above it. The narrow margin is a `max-docked:` utility, not an `sm:` one paired with a
          `docked:` twin: a `docked:` utility loses to an `sm:` twin on the same property, and the
          band `max-docked:` covers instead is one where HeroUI's own 40px rule is beaten anyway,
          since it sits in HeroUI's components layer and this is a Tailwind utility. The 40px above
          the breakpoint is then HeroUI's. The height is the window's own less those
          margins, and less the connection banner's strip, which the container also ends above
          (`style.css`): without that the strip, stacked above the dialog as well as above the
          backdrop, would cross the dialog's own bottom edge and swallow the presses landing
          there. */}
      <Frame.Container size={size === "viewer" || size === "wide" ? "lg" : size} className={size === "viewer" ? "max-docked:p-4" : undefined}>
        <Frame.Dialog
          aria-describedby={alert ? bodyId : undefined}
          className={
            size === "viewer"
              ? "control-fills h-[calc(100dvh-32px-var(--bottom-chrome-height))] w-[calc(100vw-32px)] max-w-none docked:h-[calc(100dvh-80px-var(--bottom-chrome-height))] docked:w-[min(1440px,calc(100vw-80px))]"
              : size === "wide"
                ? "control-fills max-w-3xl"
                : "control-fills"
          }
        >
          <span ref={markerRef} hidden />
          <TitledControl title={t("common.close")}>
            {/* Named explicitly so it always matches the tooltip. */}
            <Frame.CloseTrigger aria-label={t("common.close")} />
          </TitledControl>
          {onSubmit ? (
            // A key held down repeats: the press that opened the dialog (a menu item takes Enter and
            // Space on keydown) would otherwise go on to submit it before the user has read it, Enter
            // on a repeat, Space on its release. Caught on the way down, since react-aria's buttons
            // stop the keydown from bubbling. Where text is typed the repeats are the user's own, so
            // they stay: Space in any text field, Enter in a textarea.
            <form
              className="contents"
              onKeyDownCapture={(e) => {
                if (submitsFromChoice(e)) return;
                if (!e.repeat) return;
                const enter = e.key === "Enter" && !(e.target instanceof HTMLTextAreaElement);
                if (enter || (e.key === " " && !isTextEntry(e.target))) e.preventDefault();
              }}
              onSubmit={(e) => {
                e.preventDefault();
                // React bubbles a submit through portals, so a form in a dialog opened from inside
                // this one would otherwise submit this one too.
                e.stopPropagation();
                onSubmit();
              }}
            >
              {content}
            </form>
          ) : (
            content
          )}
        </Frame.Dialog>
      </Frame.Container>
    </Frame.Backdrop>
  );
}
