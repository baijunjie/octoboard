import React, { useId, useRef } from "react";

import { useT } from "../../i18n/react";
import { FieldErrorMessage } from "./FieldErrorMessage";
import { FadeOverflow } from "../text/FadeOverflow";
import { FieldClearButton } from "../controls/FieldClearButton";
import { useHandoffAfterRemoval } from "../focus/HandoffOnUnmount";
import { MarkLineBox } from "../status/MarkLineBox";
import { MotionListBox } from "../motion/MotionListBox";
import { ReasonText } from "../text/ReasonText";
import type { Reason } from "../../format/reason";
import { StatusBadge } from "../status/StatusBadge";
import { Description, Label, ListBoxItem, ListBoxItemLabel, OutsideFieldParts, Select, SelectClearButton, SelectPopover, SelectTrigger } from "../../primitives";

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
    /** A note about the option, which assistive technology reads as its description, apart from its name
     * (the label and badge), and which is not drawn. */
    detail: string;
    /** A short note drawn under the label (why a disabled option cannot be chosen, or what an option
     * stands for), hidden from assistive technology, which has `detail`, so a `reason` always comes with
     * one. */
    reason?: Reason;
  }
);

/** The fade of a disabled option's icon. Its name is not faded, as text must stay readable: it takes the secondary
 * text colour instead; that departs from HeroUI's disabled look by the user's decision. */
const FADED = "opacity-(--disabled-opacity)";

/** The colour of a disabled option's name. */
const DISABLED_NAME = "text-app-muted";

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
 * assistive technology and its `reason` the short note drawn under its label (why it cannot be
 * chosen, or what it stands for). `isDisabled` locks the whole select, and `description` is a note
 * under it. A label too long for the control fades at its clipped edge and runs as a marquee on
 * hover.
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
 * When either mark goes while it or its popover holds focus, focus moves to the trigger in the commit
 * that removes the mark, ahead of a caller's own handoff, unless that commit disables the trigger: a
 * caller that disables the select with the mark's going hands focus on itself (`useFocusHandoff`), from
 * `<body>`, where focus falls from the removed mark.
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
  locked,
  textDir,
  isLoading = false,
  description,
  describedBy,
  labelledBy,
  errorMessage,
  isInvalid,
  mark,
  labelMark,
  landing,
  variant = "secondary",
}: {
  label: string;
  options: Option<T>[];
  inline?: boolean;
  /** Which end of an input group this one sits at. */
  joined?: "start" | "end";
  hideLabel?: boolean;
  isDisabled?: boolean;
  /** Draws the disabled select as a value to read rather than a dimmed control: for a select locked with the form it
   * is in. */
  locked?: boolean;
  /** `auto` where the label and the description are a page's or a user's text, so that they take the direction of
   * their own text and align with the interface. */
  textDir?: "auto";
  /** Locks the select while its options load; an empty value shows the loading placeholder. */
  isLoading?: boolean;
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
   * which scopes the focus handoff to it. */
  mark?: (markId: string) => React.ReactElement;
  /** A `StatusMark` about the whole field, drawn after its visible label (so not with `inline`, `joined` or
   * `hideLabel`), built with the `markId` it is given as `mark` is. */
  labelMark?: (markId: string) => React.ReactElement;
  /** Makes the trigger where F6 lands in its region (`data-region-landing`). */
  landing?: boolean;
  /** HeroUI's field variant: `secondary` for a field on a surface (the default), `primary` (HeroUI's default) for one
   * on a pane's own background. */
  variant?: "primary" | "secondary";
} & (
  | { value: T; onChange: (value: T) => void; placeholder?: undefined }
  | { value: T | null; onChange: (value: T | null) => void; placeholder: string }
)): React.ReactElement {
  const t = useT();
  const shownPlaceholder = isLoading ? t("common.loading") : placeholder;
  const descriptionId = useId();
  const markId = useId();
  const labelMarkId = useId();
  const trigger = useRef<HTMLButtonElement | null>(null);
  const current = options.find((option) => option.value === value);
  const badge = current ? current.badge : isLoading ? undefined : placeholderBadge;
  const badgeInLabel = current ? current.badgeInLabel : true;
  const invalid = isInvalid === true || (errorMessage !== undefined && errorMessage !== null && errorMessage !== false && errorMessage !== "");
  const textProps = textDir === undefined ? {} : { dir: textDir, className: "align-match-parent" };
  const hasReason = options.some((option) => option.reason);
  // Decided once the commit has set the trigger's `disabled`, and before the effects of a caller's own handoff.
  const handoffFrom = useHandoffAfterRemoval(() => trigger.current);
  const handoffToTrigger = (id: string) => handoffFrom(`[data-status-mark-id="${id}"]`);
  return (
    <Select
      variant={variant}
      fullWidth={!inline && !joined}
      // A joined half that holds keyboard focus is raised, as its focus ring is drawn outside the
      // trigger and the later half would otherwise paint over it along the shared edge. The raise
      // is on the flex item, the root, which a trigger's own `z-index` would not be.
      // Both halves may carry a badge. Share the width so either name gives way within its own half.
      className={inline ? "w-48" : joined ? "min-w-0 basis-0 flex-1 has-ui-keyboard-focus:z-10" : undefined}
      label={(inline || joined || hideLabel) && !labelledBy ? label : undefined}
      labelledBy={labelledBy}
      // react-aria drops the slot's id when no note exists on mount and does not restore it when
      // children alone change. Keep the association stable for notes that appear dynamically.
      describedBy={[description ? descriptionId : undefined, describedBy].filter(Boolean).join(" ") || undefined}
      disabled={isDisabled}
      locked={locked}
      loading={isLoading}
      invalid={invalid}
      value={value}
      placeholder={shownPlaceholder}
      // Clearing arrives as a `null` key. Without a placeholder the caller has no way to take it,
      // so it is dropped there, as it always was.
      onChange={(key) => {
        if (placeholder !== undefined) (onChange as (value: T | null) => void)(key as T | null);
        else if (key !== null) onChange(key as T);
      }}
    >
      {!inline && !joined && !hideLabel && (labelMark ? (
        <div className="flex items-start gap-1">
          <Label {...textProps}>{label}</Label>
          {handoffToTrigger(labelMarkId)}
          {/* The mark is not a part of the select, as for `mark` below. */}
          <MarkLineBox size="text-sm">
            <OutsideFieldParts>{labelMark(labelMarkId)}</OutsideFieldParts>
          </MarkLineBox>
        </div>
      ) : <Label {...textProps}>{label}</Label>)}
      {/* The trigger and the mark share a box, as the mark is laid over the trigger's trailing room. */}
      <div className="relative">
        {/* Joined: the two parts square the corners they meet at and touch. Nothing is drawn between
            them: a gap shows the dialog's surface through in the light theme, and a border would sit close to this
            variant's fill in the dark one, and HeroUI's own input group draws no boundary between its parts
            either. */}
        <SelectTrigger
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
          valueClassName="flex min-w-0 flex-1 items-center gap-2 text-clip"
          value={<>
            {current?.icon && <span aria-hidden="true" className="flex shrink-0">{current.icon}</span>}
            <FadeLabel as="span" lang={current?.lang} className={!badge || badgeInLabel ? "min-w-0 flex-1" : "min-w-0"} label={current?.labelContent ?? current?.label ?? shownPlaceholder} badge={badgeInLabel ? badge : undefined} />
            {badge && !badgeInLabel && <StatusBadge>{badge}</StatusBadge>}
          </>}
        >
          {/* The room for the mark, after the value and before the clear button. */}
          {mark && <span aria-hidden="true" className="w-6 shrink-0" />}
          {placeholder !== undefined && <span aria-hidden="true" className="ms-2 w-6 shrink-0" />}
        </SelectTrigger>
        {placeholder !== undefined && (
          // The shared ghost button, as HeroUI's clear action, beside the trigger.
          <SelectClearButton onClear={() => trigger.current?.focus()}>
            <FieldClearButton disabled={isDisabled || isLoading} className="absolute end-7 top-1/2 -translate-y-1/2 ui-empty:invisible" />
          </SelectClearButton>
        )}
        {mark && (
          <>
            {handoffToTrigger(markId)}
            {/* The mark is not the select's trigger, which the select's behaviour is for. */}
            <OutsideFieldParts>
              <span className={`absolute inset-y-0 flex items-center ${placeholder !== undefined ? "end-15" : "end-7"}`}>{mark(markId)}</span>
            </OutsideFieldParts>
          </>
        )}
      </div>
      <SelectPopover
        // Options with a reason widen the popover to fit it; the others keep the trigger's width.
        placement={hasReason ? "bottom start" : undefined}
        className={hasReason ? "w-max max-w-[min(20rem,calc(100vw-2rem))]" : "w-(--trigger-width) max-w-[calc(100vw-2rem)]"}
      >
        <MotionListBox className="relative">
          {options.map((option) => (
            <ListBoxItem
              data-list-motion={option.value}
              data-marquee-scope
              // A disabled option is dimmed by its name taking the secondary text colour and its icon fading;
              // react-aria marks no option disabled under `disabledBehavior="selection"`, so nothing fades the row.
              className="pe-7"
              key={option.value}
              id={option.value}
              // Type-to-select matches on, and the hidden native option reads, "label, badge";
              // the option's own accessible name comes from its content, where the badge's hidden comma separates them.
              textValue={option.badge ? `${option.label}, ${option.badge}` : option.label}
              disabled={option.isDisabled}
              indicator
            >
              {option.icon && (
                <span aria-hidden="true" className={`flex shrink-0 ${option.isDisabled ? FADED : ""}`}>
                  {option.icon}
                </span>
              )}
              {option.detail ? (
                <span className="flex min-w-0 flex-1 flex-col">
                  {/* The option's name is its label and badge alone: the `label` slot keeps the description
                      (and the note under the label) out of it, so a screen reader does not say it twice. */}
                  <ListBoxItemLabel className="flex min-w-0 items-center gap-2">
                    <FadeLabel as="span" lang={option.lang} className={option.isDisabled ? `min-w-0 ${DISABLED_NAME}` : "min-w-0"} label={option.labelContent ?? option.label} badge={option.badgeInLabel ? option.badge : undefined} />
                    {option.badge && !option.badgeInLabel && <StatusBadge>{option.badge}</StatusBadge>}
                  </ListBoxItemLabel>
                  <Description className="sr-only">{option.detail}</Description>
                  {option.reason && <span aria-hidden="true" className="text-xs text-app-muted"><ReasonText reason={option.reason} /></span>}
                </span>
              ) : (
                <FadeLabel lang={option.lang} className={`${option.badge && !option.badgeInLabel ? "min-w-0" : "min-w-0 flex-1"}${option.isDisabled ? ` ${DISABLED_NAME}` : ""}`} label={option.labelContent ?? option.label} badge={option.badgeInLabel ? option.badge : undefined} />
              )}
              {option.badge && !option.badgeInLabel && !option.detail && <StatusBadge>{option.badge}</StatusBadge>}
            </ListBoxItem>
          ))}
        </MotionListBox>
      </SelectPopover>
      {description && <Description id={descriptionId} {...textProps}>{description}</Description>}
      {errorMessage !== undefined && (
        <FieldErrorMessage>{invalid ? errorMessage : null}</FieldErrorMessage>
      )}
    </Select>
  );
}
