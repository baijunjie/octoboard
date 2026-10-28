import { AlertDialog, Modal } from "@heroui/react";
import React, { useEffect, useId, useLayoutEffect, useRef, useState } from "react";

import { Button } from "../components/Button";
import { takeMenuFocusToRestore } from "../components/ActionMenu";
import { FadedScroller, useFadeRegion } from "../components/scrollFade";
import { TitledControl } from "../components/TitledControl";
import { useLatest } from "../components/useLatest";
import { useT } from "../i18n/react";
import type { AcknowledgementView } from "../errorAlerts";
import { reportError } from "../reportError";
import { dialogAround } from "./dialogElement";
import { ErrorAlertBody, alertTitle } from "./ErrorAlertBody";
import { useEscapeWhileTooltipOpen } from "./useEscapeWhileTooltipOpen";
import { useHeightMotion } from "./useHeightMotion";
import { useToastClearance } from "./useToastClearance";
import { useTopLayerAtMount } from "./useTopLayerAtMount";

/** The busy state every dialog that talks to the daemon needs. Execution failures go to the
 * acknowledgement queue; what is wrong with a single field goes to
 * that field's own `errorMessage` instead (see `useSubmitValidation`). `run` marks the dialog busy
 * for the duration of `action` and reports a rejection to the queue instead of letting it escape,
 * so the dialog stays open for another try. A dialog reused for successive subjects passes a
 * `resetKey` naming the current one: when it changes the busy state is cleared, and an action
 * still running for the previous subject no longer reports into it. A failure is titled
 * `errorTitle`, the plain name of what failed. */
export function useDialogAction(resetKey: string | undefined, errorTitle: string): {
  busy: boolean;
  run: (action: () => Promise<void>) => Promise<void>;
} {
  const [busy, setBusy] = useState(false);
  const [seenKey, setSeenKey] = useState(resetKey);
  const currentKey = useLatest(resetKey);
  if (seenKey !== resetKey) {
    setSeenKey(resetKey);
    setBusy(false);
  }

  const run = async (action: () => Promise<void>) => {
    const startedFor = resetKey;
    const stale = () => currentKey.current !== startedFor;
    setBusy(true);
    try {
      await action();
    } catch (err) {
      if (!stale()) reportError(err as Error, undefined, undefined, undefined, errorTitle);
    } finally {
      if (!stale()) setBusy(false);
    }
  };

  return { busy, run };
}

/** Puts keyboard focus back on the element `target` returns when `deps` change and focus has fallen to `<body>`,
 * which is where it goes when the focused control is unmounted by the change. Left alone, a dialog's
 * Escape handling and Tab containment stop working, since they hang off focus being inside it.
 *
 * Never at mount, StrictMode's second run included: a dialog opened by a press without a pointer (assistive
 * technology, a script) has no focus yet then, since react-aria puts off its opening focus (`autoFocus`, or the dialog
 * itself) until transitions end and drops it if focus has moved meanwhile, so focusing anything here would take the
 * dialog's opening focus away from its first field. */
export function useRefocusIfLost(target: () => HTMLElement | null | undefined, deps: React.DependencyList): void {
  const seen = useRef(deps);
  useEffect(() => {
    const changed = deps.some((dep, index) => !Object.is(dep, seen.current[index]));
    seen.current = deps;
    if (changed && document.activeElement === document.body) target()?.focus();
  }, deps);
}

/** Moves keyboard focus from `button` to `fallback` when `disabled` turns true while the button holds it: a
 * natively disabled button cannot keep focus, and the dialog's Escape and Tab containment hang off focus
 * being inside it. WebKit blurs the button to `<body>` as it becomes disabled, before this runs, so the
 * button counts as having held focus when `<body>` has it and the button had it last. A layout effect, so
 * focus has moved before the change is painted. */
export function useFocusLeavingWhenDisabled(
  button: React.RefObject<HTMLElement | null>,
  disabled: boolean,
  fallback: React.RefObject<HTMLElement | null>,
): void {
  const held = useRef(false);
  useLayoutEffect(() => {
    held.current = document.activeElement === button.current;
    const track = (event: FocusEvent) => {
      if (event.target !== button.current) return;
      // A blur that arrives once the button is disabled is the browser's doing, not the user moving on.
      if (event.type === "focusin") held.current = true;
      else if (!(event.target as HTMLButtonElement).disabled) held.current = false;
    };
    document.addEventListener("focusin", track);
    document.addEventListener("focusout", track);
    return () => {
      document.removeEventListener("focusin", track);
      document.removeEventListener("focusout", track);
    };
  }, []);
  useLayoutEffect(() => {
    if (!disabled) return;
    const active = document.activeElement;
    if (button.current && (active === button.current || ((active === document.body || !active) && held.current))) fallback.current?.focus();
    held.current = false;
  }, [disabled]);
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
 * The frame every dialog shares. Unless `dismissable` is false, Escape calls `onClose`.
 * A click outside also closes a non-alert dialog; confirmations require an explicit response. The dialog is
 * mounted only while it is open, so `isOpen` is constant. With `onSubmit` the header, body and footer sit in
 * a `<form>`, so Enter in a text field submits it — the dialog decides what "submit" means. The
 * footer is Cancel plus a submit button labelled `submitLabel`, unless the dialog brings its own
 * `footer`. While `busy`, the submit button is marked pending rather than disabled: a disabled
 * attribute would blur the button the user has just pressed and take the dialog's Escape and Tab
 * with it for as long as the request runs, where pending blocks press and hover, keeps the button
 * focusable and announces itself. Cancel is disabled instead, which costs nothing because focus
 * cannot be on it at the moment the request starts. `submitDisabled` disables the submit button
 * alone, before any action, Cancel stays pressable, and since it is the form's only submit control
 * this also suppresses implicit submission from Enter in a text field. Focus is never left on a button that
 * has become disabled: it moves to Cancel. A form dialog puts `autoFocus` on its first text field, so Enter
 * submits at once; one with no text field passes `autoFocusSubmit` to open on the submit button instead, or on
 * Cancel when there is nothing to submit yet (a dialog with its own `footer` puts `autoFocus` on its own
 * button). `Button` and `TextInput` mark that control with `openingFocus`, which is where focus comes back to when an
 * acknowledged alert leaves it nowhere to return (`focusAfterLoss`); a field built from HeroUI's own parts marks itself.
 */
function DialogFrame({
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
  failing,
  resetKey,
  dismissable = true,
  focusesItselfOnOpen,
  topLayerAtMount,
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
  /** The dialog is a failure modal, announced and described as an alert dialog whatever frame it is in. */
  failing?: boolean;
  /** An error requires its acknowledgement button, with no implicit dismissal. */
  dismissable?: boolean;
  /** Names what a dialog reused across subjects currently asks about; when it changes and focus
   * was lost with the old content, it is put back on the dialog. */
  resetKey?: string;
  /** The caller puts focus on the dialog itself, in the render after the one that mounted it, once react-aria has
   * turned the modal's focus containment on (from a layout effect at mount, which forces a render), so the frame never
   * puts focus found on `<body>` back on the dialog when `resetKey` changes: a caller that passes this
   * with a `resetKey` puts focus back itself for each new subject. Until then the frame holds focus on an empty
   * element inside the dialog from its mount: react-aria takes focus off a dialog that finds focus outside it when the
   * mount's passive effects run, half a second later, and puts it back only once no transition is running on the page,
   * so one that never reports its end (in Chrome, one still running inside a disclosure as it folds away) would leave
   * focus on the page body. The empty element rather than the dialog itself, for a modal that mounts in the same commit
   * (one that a dialog opens from an effect as it mounts): with the dialog itself focused at mount, focus ends on this
   * dialog's first control or on that modal's opening control, while focus moved on from the empty element stays put. A
   * dialog already open underneath plays no part in that while this one is also `topLayerAtMount`, as react-aria never
   * counts focus inside a top layer as escaping its containment. */
  focusesItselfOnOpen?: boolean;
  /** Marks the overlay, the element portalled to `<body>`, a react-aria top layer (`data-react-aria-top-layer`) for
   * the commit it mounts in, and only that commit (see `useTopLayerAtMount`). For a modal that comes up a commit after
   * what raised it, which can be the very commit another modal mounts in: one that a dialog opens from an effect as it
   * mounts. */
  topLayerAtMount?: boolean;
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
      // Decided a turn later: a dialog that replaces this one in the same commit (the login dialog's running form
      // giving way to its failure form) has mounted by then, so the menu's focus is not taken from it.
      setTimeout(() => {
        if (openDialogs > 0) return;
        const element = takeMenuFocusToRestore();
        if (element?.isConnected) element.focus();
      }, 0);
    };
  }, []);

  const markerRef = useRef<HTMLSpanElement>(null);
  const submitRef = useRef<HTMLButtonElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);
  useFocusLeavingWhenDisabled(submitRef, submitDisabled === true, cancelRef);
  useRefocusIfLost(() => (focusesItselfOnOpen ? null : dialogAround(markerRef)), [resetKey]);
  const landingRef = useRef<HTMLSpanElement>(null);
  useLayoutEffect(() => {
    landingRef.current?.focus({ preventScroll: true });
  }, []);
  useToastClearance(markerRef, footer !== null);
  useHeightMotion(markerRef);
  useEscapeWhileTooltipOpen(() => dialogAround(markerRef), dismissable ? onClose : () => {});
  const topLayer = useTopLayerAtMount(topLayerAtMount === true, markerRef);

  const Frame = alert ? AlertDialog : Modal;
  const alerting = alert || failing;
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
      {/* HeroUI's body is muted grey; prose in a dialog reads in the foreground, and a note that is
          secondary says so itself (`Description`, `text-app-muted`). */}
      {/* The ring of the focusable scroller is drawn on its parent: the fade is a mask on the scroller
            and clips by its border box, so a ring on the scroller itself, which sits outside that box, is
            cut off. The ring is the project's usual one (2px `--focus`), from the scroller's own state. */}
      <Frame.Body
        id={bodyId}
        className={`text-foreground ${size === "viewer" ? `relative m-0 flex flex-col overflow-hidden p-0 ${region.focused ? "after:pointer-events-none after:absolute after:inset-0 after:z-10 after:ring-2 after:ring-focus after:ring-inset" : ""}` : `flex flex-col overflow-y-visible rounded-lg ${region.focused ? "ring-2 ring-focus" : ""}`}`}
      >
        <FadedScroller region={region} className={size === "viewer" ? "flex min-h-0 flex-1 flex-col" : "flex min-h-0 flex-1 flex-col gap-4 p-1"}>
          {children}
          {/* Out of the flow, so the body's gap leaves it out, and hidden from assistive technology, so the virtual
              cursor does not stop on it: it holds focus only within the task that mounts the dialog. */}
          {focusesItselfOnOpen && <span ref={landingRef} tabIndex={-1} aria-hidden="true" className="absolute" />}
        </FadedScroller>
      </Frame.Body>
      {footer !== null && (
        <Frame.Footer className="flex-wrap">
          {footer ?? (
            <>
              <Button ref={cancelRef} type="button" variant="secondary" onPress={onClose} isDisabled={busy} autoFocus={autoFocusSubmit && submitDisabled}>
                {t("common.cancel")}
              </Button>
              <Button ref={submitRef} type="submit" isPending={busy} isDisabled={submitDisabled} autoFocus={autoFocusSubmit && !submitDisabled}>
                {submitLabel}
              </Button>
            </>
          )}
        </Frame.Footer>
      )}
    </>
  );

  const heading = <Frame.Heading className="min-w-0 [overflow-wrap:anywhere]">{title as BodyChildren}</Frame.Heading>;
  // Keep the modal parts together: HeroUI spaces them through adjacent-sibling selectors.
  const content = (
    <>
      {/* HeroUI places the close button over the header's end without reserving room for it, so
          the heading is padded clear of it; a long title (a session's name, say) has to wrap
          anywhere, since a name with no place to break would run out of the dialog. */}
      <Frame.Header className={`${size === "viewer" ? "mx-6 mt-6 " : ""}${headerLeading ? "grid grid-cols-[auto_minmax(0,1fr)] items-center gap-x-2 gap-y-1 pe-6" : subtitle ? "gap-1 pe-6" : "pe-6"}`}>
        {headerLeading}
        {heading}
        {subtitle}
      </Frame.Header>
      {body}
    </>
  );

  return (
    <Frame.Backdrop
      isOpen
      isDismissable={dismissable && !alert}
      isKeyboardDismissDisabled={!dismissable}
      onOpenChange={(open) => !open && dismissable && onClose()}
      data-react-aria-top-layer={topLayer ? "true" : undefined}
    >
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
          {...(failing && { role: "alertdialog" })}
          aria-describedby={alerting ? bodyId : undefined}
          className={
            size === "viewer"
              ? "control-fills overflow-hidden p-0 h-[calc(100dvh-32px-var(--bottom-chrome-height))] w-[calc(100vw-32px)] max-w-none docked:h-[calc(100dvh-80px-var(--bottom-chrome-height))] docked:w-[min(1440px,calc(100vw-80px))]"
              : size === "wide"
                ? "control-fills max-w-3xl"
                : "control-fills"
          }
        >
          <span ref={markerRef} hidden />
          {dismissable && <TitledControl title={t("common.close")}>
            {/* Named explicitly so it always matches the tooltip. */}
            <Frame.CloseTrigger aria-label={t("common.close")} />
          </TitledControl>}
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

type DialogFrameProps = Parameters<typeof DialogFrame>[0];

/**
 * The frame every dialog shares (see `DialogFrame`'s props). With `failure` the dialog turns into a failure modal in
 * place: a failure that ends the dialog's purpose replaces its content, in the same modal, with the error alert
 * dialog's form — the danger icon, the failure's title, its explanation and "Error details", and the acknowledgement
 * as the only button, which calls `onClose`. It is an acknowledgement-only failure: there is no Report issue button,
 * and the type of `failure` leaves out an unexpected failure, whose report-a-bug guidance would have nothing to act on
 * here. Only that acknowledgement closes it. The dialog's own title, body, footer and submit are not shown.
 */
export function Dialog({ failure, ...props }: DialogFrameProps & {
  failure?: AcknowledgementView;
}): React.ReactElement {
  const t = useT();
  if (!failure) return <DialogFrame {...(props as DialogFrameProps)} />;
  return (
    <DialogFrame
      failing
      size={props.size}
      alert={props.alert}
      topLayerAtMount={props.topLayerAtMount}
      resetKey={failure.message}
      dismissable={false}
      onClose={props.onClose}
      headerLeading={<AlertDialog.Icon status="danger" />}
      title={alertTitle(t, failure)}
      footer={<Button autoFocus variant="danger" onPress={props.onClose}>{t("error.acknowledge")}</Button>}
    >
      <ErrorAlertBody alert={failure} />
    </DialogFrame>
  );
}
