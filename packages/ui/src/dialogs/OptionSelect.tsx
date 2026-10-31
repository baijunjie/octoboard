import { Description, FieldError, Label, ListBox, Select } from "@heroui/react";
import React, { useId, useRef } from "react";
import { ButtonContext } from "react-aria-components";

import { CodeToneContext } from "../components/codeTone";
import { FadeOverflow } from "../components/FadeOverflow";
import { FieldClearButton } from "../components/FieldClearButton";
import { HandOffOnUnmount } from "../components/HandOffOnUnmount";
import { MarkLineBox } from "../components/MarkLineBox";
import { MotionListBox } from "../components/MotionListBox";
import { ReasonText, type Reason } from "../components/ReasonText";
import { StatusBadge } from "../components/StatusBadge";

/** One choice of an `OptionSelect`. */
export type Option<T extends string> = {
  value: T;
  label: string;
  /** Visible whole-label presentation; matching and hidden native options retain the raw label. */
  labelContent?: React.ReactNode;
  lang?: string;
  icon?: React.ReactNode;
  /** A status badge after the label ("Not added"), which never shrinks: the label gives way first (unless `badgeInLabel`). */
  badge?: string;
  /** Draws `badge` inside the label's fade, so the two give way together: for a badge that completes
   * the label ("Same as console" and the value that resolves to) rather than qualifying it. */
  badgeInLabel?: boolean;
  isDisabled?: boolean;
} & (
  | { detail?: undefined; reason?: undefined }
  | {
    /** A note about the option, which assistive technology reads as its description and which is not drawn. */
    detail: string;
    /** A short note drawn under the label (why a disabled option cannot be chosen), hidden from assistive
     * technology, which has `detail`, so a `reason` always comes with one. */
    reason?: Reason;
  }
);

/** The fade of a disabled option's icon. Its name is not faded, as text must stay readable: it takes the secondary
 * text colour instead; that departs from HeroUI's disabled look by the user's decision. */
const FADED = "opacity-(--disabled-opacity)";

/** The colour of a disabled option's name. */
const DISABLED_NAME = "text-app-muted";

/** react-aria-components drops an `aria-disabled` given to a list box item, and under `disabledBehavior="selection"`
 * sets none itself, so a disabled option is announced as one that can be chosen; it is set on the element here. */
function markDisabled(node: HTMLElement | null, disabled: boolean | undefined) {
  if (!node) return;
  if (disabled) node.setAttribute("aria-disabled", "true");
  else node.removeAttribute("aria-disabled");
}

/** An option's label, followed by its badge when the badge is drawn inside the label's fade. The two sit in a row
 * centred on one line, as an inline pill aligned to the baseline would sit off-centre beside CJK text and lengthen
 * the line it is on, and with it the trigger. In the tooltip of a clipped label only the badge wraps (the label stays
 * on one line), as the tooltip's width is bounded; outside it nothing shrinks, so the row overflows and the fade clips
 * it. */
function OptionText({ label, badge, inTooltip }: { label: React.ReactNode; badge?: string; inTooltip?: boolean }): React.ReactElement {
  if (!badge) return <>{label}</>;
  return (
    <span className={inTooltip ? "flex items-center gap-2" : "inline-flex items-center gap-2 align-top"}>
      <span className="shrink-0">{label}</span>
      <StatusBadge dir="auto" overflow={inTooltip ? "wrap" : undefined}>{badge}</StatusBadge>
    </span>
  );
}

/** An option's label in its fade, with the badge inside it when there is one, and the same text as the
 * tooltip of a clipped label. */
function FadeLabel({ label, badge, lang, className, as }: {
  label: React.ReactNode;
  badge?: string;
  lang?: string;
  className: string;
  as?: "span";
}): React.ReactElement {
  return (
    <FadeOverflow as={as} lang={lang} dir="auto" className={className} titleWhenClipped={<OptionText label={label} badge={badge} inTooltip />}>
      <OptionText label={label} badge={badge} />
    </FadeOverflow>
  );
}

/** A single-choice dropdown over a fixed list of options. An option's `lang` marks its text as being
 * in a language other than the UI's; its `icon` is drawn before its text, in the list and in the
 * trigger while it is the chosen one. An option with `isDisabled` is shown, named, but cannot be
 * chosen — the current `value` may still be one, so the reason it cannot be changed away *to* is on
 * screen without taking away what is already picked. An option's `detail` is its description for
 * assistive technology and its `reason` the short note drawn under its label (why an option is
 * disabled). `isDisabled` locks the whole select, and `description` is a note under it. A label too
 * long for the control fades at its clipped edge and runs as a marquee on hover.
 *
 * With `inline` it is only as wide as its value and `label` is its accessible name alone, for a
 * control whose own row already shows the name (a setting). With `joined` it is one part of an
 * input group rather than a field of its own: it shares the group's label, so `label` is its
 * accessible name alone, it takes an equal share of the group's width, and the corners it meets
 * its neighbour at are squared and the two triggers meet with nothing between them: each half's own
 * chevron and hover fill are what say there are two controls.
 *
 * With `hideLabel` the field is full width like any other but `label` is its accessible name alone,
 * for a select repeated down a list whose rows already show what each one is about. With
 * `labelledBy` as well, those elements name it instead, in the order given.
 *
 * With `mark`, given the `markId` its `StatusMark` has to carry, a warning or an error about the field's
 * current value stands inside it at its trailing end, before the clear button. The trigger is a button,
 * which cannot hold another, so the mark is laid over the room the trigger keeps free for it. With
 * `labelMark`, built likewise, one about the whole field (its list of options) stands right after the
 * label, beside it rather than inside it, so that the field's accessible name stays the label's text.
 * When either mark goes while it or its popover holds focus, focus moves to the trigger once the render
 * is committed, unless the trigger is disabled by then: a caller that disables the select with the
 * mark's going hands focus on itself (`useFocusHandoff`), from `<body>`, where focus falls from the
 * removed mark.
 *
 * With `errorMessage` the field is marked invalid and says what is wrong with it right under the select, in its
 * `FieldError`, as `TextInput` does. A `joined` half is not a field of its own: the group draws the message, and the
 * half only takes `isInvalid` and the `describedBy` of the message.
 *
 * With `placeholder` the choice is optional: `value` may be `null`, which shows the placeholder
 * instead of an option and stands for "nothing chosen", and once something is chosen a clear
 * button (and Backspace or Delete on the focused control) puts it back to `null`, passed to
 * `onChange`. */
export function OptionSelect<T extends string>({
  label,
  options,
  value,
  onChange,
  placeholder,
  placeholderBadge,
  inline,
  joined,
  hideLabel,
  isDisabled,
  description,
  describedBy,
  labelledBy,
  errorMessage,
  isInvalid,
  mark,
  labelMark,
  landing,
}: {
  label: string;
  options: Option<T>[];
  inline?: boolean;
  /** Which end of an input group this one sits at. */
  joined?: "start" | "end";
  hideLabel?: boolean;
  isDisabled?: boolean;
  /** Extra explanation shown under the field. */
  description?: React.ReactNode;
  /** The id of an element elsewhere on the page that also describes the field. */
  describedBy?: string;
  /** With `hideLabel`, the ids of elements elsewhere on the page that name the field. */
  labelledBy?: string;
  /** Marks the field invalid and says what is wrong with it under the field. */
  errorMessage?: React.ReactNode;
  /** Marks the field invalid where something else draws the message (`errorMessage` implies it). */
  isInvalid?: boolean;
  /** A status badge after the placeholder, for a placeholder that names what the empty choice stands for.
   * It completes the placeholder, so it is drawn as an option's `badgeInLabel` badge is. */
  placeholderBadge?: string;
  /** A `StatusMark` about the field's current value, drawn inside it, built with the `markId` it is given,
   * which scopes the focus hand-off to it. */
  mark?: (markId: string) => React.ReactElement;
  /** A `StatusMark` about the whole field, drawn after its visible label (so not with `inline`, `joined` or
   * `hideLabel`), built with the `markId` it is given as `mark` is. */
  labelMark?: (markId: string) => React.ReactElement;
  /** Makes the trigger where F6 lands in its region (`data-region-landing`). */
  landing?: boolean;
} & (
  | { value: T; onChange: (value: T) => void; placeholder?: undefined }
  | { value: T | null; onChange: (value: T | null) => void; placeholder: string }
)): React.ReactElement {
  const descriptionId = useId();
  const markId = useId();
  const labelMarkId = useId();
  const trigger = useRef<HTMLButtonElement | null>(null);
  const current = options.find((option) => option.value === value);
  const badge = current ? current.badge : placeholderBadge;
  const badgeInLabel = current ? current.badgeInLabel : true;
  const invalid = isInvalid === true || (errorMessage !== undefined && errorMessage !== null && errorMessage !== false && errorMessage !== "");
  const hasReason = options.some((option) => option.reason);
  // The hand-off runs before the commit sets the sibling trigger's `disabled`, so the trigger is asked
  // after it: one disabled in the same render is not focused (WebKit would blur it as it is disabled)
  // and focus falls to `<body>` for the caller to take, and focus a caller has placed meanwhile stays.
  const handOffToTrigger = (id: string) => (
    <HandOffOnUnmount from={`[data-status-mark-id="${id}"]`} to={() => queueMicrotask(() => { const target = trigger.current; if (target?.isConnected && !target.disabled && (!document.activeElement || document.activeElement === document.body)) target.focus(); })} />
  );
  return (
    // HeroUI's variant for a field on a surface: in a dialog, or in the Processes panel's toolbar.
    <Select
      variant="secondary"
      fullWidth={!inline && !joined}
      // A joined half that holds keyboard focus is raised, as its focus ring is drawn outside the
      // trigger and the later half would otherwise paint over it along the shared edge. The raise
      // is on the flex item, the root, which a trigger's own `z-index` would not be.
      // Both halves may carry a badge. Share the width so either name gives way within its own half.
      className={inline ? "w-48" : joined ? "min-w-0 basis-0 flex-1 has-[[data-focus-visible=true]]:z-10" : undefined}
      aria-label={(inline || joined || hideLabel) && !labelledBy ? label : undefined}
      aria-labelledby={labelledBy}
      // react-aria drops the slot's id when no note exists on mount and does not restore it when
      // children alone change. Keep the association stable for notes that appear dynamically.
      aria-describedby={[description ? descriptionId : undefined, describedBy].filter(Boolean).join(" ") || undefined}
      isDisabled={isDisabled}
      isInvalid={invalid}
      // As in `TextInput`: whether the dialog goes ahead is its own decision, not the browser's.
      validationBehavior="aria"
      value={value}
      placeholder={placeholder}
      // Clearing arrives as a `null` key. Without a placeholder the caller has no way to take it,
      // so it is dropped there, as it always was.
      onChange={(key) => {
        if (placeholder !== undefined) (onChange as (value: T | null) => void)(key as T | null);
        else if (key !== null) onChange(key as T);
      }}
      // react-aria's keyboard delegate defaults to skipping a disabled item in arrow-key traversal
      // (`disabledBehavior="all"`), which would make the reason a user cannot pick it invisible to
      // anyone not reading the popover by mouse or in screen-reader browse mode. `"selection"`
      // keeps it focusable and announced while still refusing the choice. It belongs on the
      // `Select`, not the `ListBox`: the list renders the state the `Select` builds
      // (`useSelectState`) and ignores its own. Neither HeroUI's nor react-aria's `SelectProps`
      // declares it, though `useSelectState` passes its props through to react-stately's
      // `useListState`, which reads it — hence the cast.
      {...({ disabledBehavior: "selection" } as React.ComponentProps<typeof Select>)}
    >
      {!inline && !joined && !hideLabel && (labelMark ? (
        <div className="flex items-start gap-1">
          <Label>{label}</Label>
          {handOffToTrigger(labelMarkId)}
          {/* The select's `ButtonContext` is for its trigger; reset around the mark as for `mark` below. */}
          <MarkLineBox size="text-sm">
            <ButtonContext.Provider value={{}}>{labelMark(labelMarkId)}</ButtonContext.Provider>
          </MarkLineBox>
        </div>
      ) : <Label>{label}</Label>)}
      {/* The trigger and the mark share a box, as the mark is laid over the trigger's trailing room. */}
      <div className="relative">
        {/* Joined: the two parts square the corners they meet at and touch. Nothing is drawn between
            them: a gap shows the dialog's surface through at about 1.3:1 in the light theme, and a
            border would sit close to this variant's fill in the dark one, and HeroUI's own input
            group draws no boundary between its parts either. */}
        <Select.Trigger
          ref={trigger}
          data-marquee-scope
          data-region-landing={landing || undefined}
          className={
            joined === "start"
              ? "w-full flex-1 rounded-e-none"
              : joined === "end"
                ? "w-full flex-1 rounded-s-none"
                : "w-full"
          }
        >
          <Select.Value className="flex min-w-0 flex-1 items-center gap-2 text-clip">
            {() => <>
              {current?.icon && <span aria-hidden="true" className="flex shrink-0">{current.icon}</span>}
              <FadeLabel as="span" lang={current?.lang} className={!badge || badgeInLabel ? "min-w-0 flex-1" : "min-w-0"} label={current?.labelContent ?? current?.label ?? placeholder} badge={badgeInLabel ? badge : undefined} />
              {badge && !badgeInLabel && <StatusBadge>{badge}</StatusBadge>}
            </>}
          </Select.Value>
          {/* The room for the mark, after the value and before the clear button. */}
          {mark && <span aria-hidden="true" className="w-6 shrink-0" />}
          {placeholder !== undefined && <span aria-hidden="true" className="ms-2 w-6 shrink-0" />}
          <Select.Indicator />
        </Select.Trigger>
        {placeholder !== undefined && (
          // A real button must be a sibling of the trigger, never nested inside it. Keep HeroUI's
          // clear action and shortcut registration while drawing the shared ghost button.
          <ButtonContext.Provider value={{}}>
            <Select.ClearButton<"button">
              onClick={() => trigger.current?.focus()}
              render={(props) => (
                <FieldClearButton
                  {...props}
                  aria-hidden={undefined}
                  isDisabled={isDisabled}
                  className="absolute end-7 top-1/2 -translate-y-1/2 data-[empty=true]:invisible"
                />
              )}
            />
          </ButtonContext.Provider>
        )}
        {mark && (
          <>
            {handOffToTrigger(markId)}
            {/* The select gives its trigger's behaviour to every button inside it through `ButtonContext`;
                the mark is not that button, so it is reset around the mark. The select's other contexts (text,
                label, list state) are not reset and still reach the popover's body, which holds no such parts. */}
            <ButtonContext.Provider value={{}}>
              <span className={`absolute inset-y-0 flex items-center ${placeholder !== undefined ? "end-15" : "end-7"}`}>{mark(markId)}</span>
            </ButtonContext.Provider>
          </>
        )}
      </div>
      <Select.Popover
        // Options with a reason widen the popover to fit it; the others keep the trigger's width.
        placement={hasReason ? "bottom start" : undefined}
        className={hasReason ? "w-max max-w-[min(20rem,calc(100vw-2rem))]" : "w-(--trigger-width) max-w-[calc(100vw-2rem)]"}
      >
        <MotionListBox className="relative">
          {options.map((option) => (
            <ListBox.Item
              data-list-motion={option.value}
              data-marquee-scope
              // A disabled option is dimmed by its name taking the secondary text colour and its icon fading;
              // react-aria marks no option disabled under `disabledBehavior="selection"`, so nothing fades the row.
              className="pe-7"
              // A new callback each render, so the attribute follows `isDisabled` as it changes.
              ref={(node: HTMLElement | null) => markDisabled(node, option.isDisabled)}
              key={option.value}
              id={option.value}
              // Type-to-select matches on, and the hidden native option reads, "label, badge";
              // the option's own accessible name comes from its content, where the badge's hidden comma separates them.
              textValue={option.badge ? `${option.label}, ${option.badge}` : option.label}
              isDisabled={option.isDisabled}
            >
              {option.icon && (
                <span aria-hidden="true" className={`flex shrink-0 ${option.isDisabled ? FADED : ""}`}>
                  {option.icon}
                </span>
              )}
              {option.detail ? (
                <span className={`flex min-w-0 flex-col ${option.badge && !option.badgeInLabel ? "" : "flex-1"}`}>
                  <FadeLabel lang={option.lang} className={option.isDisabled ? `min-w-0 ${DISABLED_NAME}` : "min-w-0"} label={option.labelContent ?? option.label} badge={option.badgeInLabel ? option.badge : undefined} />
                  <Description className="sr-only">{option.detail}</Description>
                  {option.reason && <span aria-hidden="true" className="text-xs text-app-muted"><ReasonText reason={option.reason} /></span>}
                </span>
              ) : (
                <FadeLabel lang={option.lang} className={`${option.badge && !option.badgeInLabel ? "min-w-0" : "min-w-0 flex-1"}${option.isDisabled ? ` ${DISABLED_NAME}` : ""}`} label={option.labelContent ?? option.label} badge={option.badgeInLabel ? option.badge : undefined} />
              )}
              {option.badge && !option.badgeInLabel && <StatusBadge>{option.badge}</StatusBadge>}
              <ListBox.ItemIndicator />
            </ListBox.Item>
          ))}
        </MotionListBox>
      </Select.Popover>
      {description && <Description id={descriptionId}>{description}</Description>}
      {errorMessage !== undefined && (
        // Renders nothing while the field is valid; the alert is on an element of its own because react-aria drops a
        // `role` given to the error itself (see `TextInput`).
        <FieldError>
          <CodeToneContext.Provider value="danger">
            <span role="alert">{invalid ? errorMessage : null}</span>
          </CodeToneContext.Provider>
        </FieldError>
      )}
    </Select>
  );
}
