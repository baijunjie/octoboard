import { Description, Label, ListBox, Select } from "@heroui/react";
import React, { useId } from "react";

import { FadeOverflow } from "../components/FadeOverflow";
import { MotionListBox } from "../components/MotionListBox";
import { ReasonText, type Reason } from "../components/ReasonText";
import { StatusBadge } from "../components/StatusBadge";

/** One choice of an `OptionSelect`. */
export type Option<T extends string> = {
  value: T;
  label: string;
  lang?: string;
  icon?: React.ReactNode;
  /** A status badge after the label ("Not added"), which never shrinks: the label gives way first. */
  badge?: string;
  isDisabled?: boolean;
} & (
  | { detail?: undefined; reason?: undefined }
  | {
    /** A note about the option, which assistive technology reads as its description and which is not drawn. */
    detail: string;
    /** A short note drawn under the label (why a disabled option cannot be chosen), hidden from assistive
     * technology, which has `detail`, so a `reason` always comes with one. On a disabled option it is not
     * faded with the rest of the row, so it stays readable. */
    reason?: Reason;
  }
);

/** The fade of a disabled option's name and icon, which the row itself keeps off when it has a reason to show. */
const FADED = "opacity-(--disabled-opacity)";

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
 * accessible name alone, it takes an equal share of the group's width (the start half whose value
 * carries a badge keeps its natural width, up to 70%, and the other half gives way), and the corners it meets
 * its neighbour at are squared and the two triggers meet with nothing between them: each half's own
 * chevron and hover fill are what say there are two controls.
 *
 * With `hideLabel` the field is full width like any other but `label` is its accessible name alone,
 * for a select repeated down a list whose rows already show what each one is about. With
 * `labelledBy` as well, those elements name it instead, in the order given.
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
  descriptionDetails,
  describedBy,
  labelledBy,
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
  descriptionDetails?: React.ReactNode;
  /** The id of an element elsewhere on the page that also describes the field. */
  describedBy?: string;
  /** With `hideLabel`, the ids of elements elsewhere on the page that name the field. */
  labelledBy?: string;
  /** A status badge after the placeholder, for a placeholder that names what the empty choice stands for.
   * Unlike an option's badge it gives way when it is long, clipped with an ellipsis before the clear button. */
  placeholderBadge?: string;
} & (
  | { value: T; onChange: (value: T) => void; placeholder?: undefined }
  | { value: T | null; onChange: (value: T | null) => void; placeholder: string }
)): React.ReactElement {
  const descriptionId = useId();
  const current = options.find((option) => option.value === value);
  const badge = current ? current.badge : placeholderBadge;
  const hasReason = options.some((option) => option.reason);
  return (
    // HeroUI's variant for a field on a surface: every one of these sits in a dialog.
    <Select
      variant="secondary"
      fullWidth={!inline && !joined}
      // A joined half that holds keyboard focus is raised, as its focus ring is drawn outside the
      // trigger and the later half would otherwise paint over it along the shared edge. The raise
      // is on the flex item, the root, which a trigger's own `z-index` would not be.
      // The start half of a joined pair whose value carries a badge keeps its natural width, so the name and
      // the badge stay readable and the other half is what gives way.
      className={inline ? "w-48" : joined ? `min-w-0 has-[[data-focus-visible=true]]:z-10 ${joined === "start" && badge ? "flex-none max-w-[70%]" : "flex-1"}` : undefined}
      aria-label={(inline || joined || hideLabel) && !labelledBy ? label : undefined}
      aria-labelledby={labelledBy}
      // react-aria drops the slot's id when no note exists on mount and does not restore it when
      // children alone change. Keep the association stable for notes that appear dynamically.
      aria-describedby={[description ? descriptionId : undefined, describedBy].filter(Boolean).join(" ") || undefined}
      isDisabled={isDisabled}
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
      {!inline && !joined && !hideLabel && <Label>{label}</Label>}
      {/* Joined: the two parts square the corners they meet at and touch. Nothing is drawn between
          them: a gap shows the dialog's surface through at about 1.3:1 in the light theme, and a
          border would sit close to this variant's fill in the dark one, and HeroUI's own input
          group draws no boundary between its parts either. */}
      <Select.Trigger
        data-marquee-scope
        className={
          joined === "start"
            ? "w-full rounded-e-none"
            : joined === "end"
              ? "w-full rounded-s-none"
              : undefined
        }
      >
        <Select.Value className="flex min-w-0 flex-1 items-center gap-2 text-clip">
          {() => <>
            {current?.icon && <span aria-hidden="true" className="flex shrink-0">{current.icon}</span>}
            <FadeOverflow as="span" lang={current?.lang} dir="auto" className={!badge ? "min-w-0 flex-1" : current ? "min-w-0" : "shrink-0"} titleWhenClipped={current?.label ?? placeholder}>
              {current?.label ?? placeholder}
            </FadeOverflow>
            {/* A placeholder's own words are short and say what the field does, so its badge (an
                inherited value that can be a long model id) is what gives way. */}
            {badge && <StatusBadge overflow={current ? undefined : "truncate"}>{badge}</StatusBadge>}
          </>}
        </Select.Value>
        {placeholder !== undefined && <Select.ClearButton />}
        <Select.Indicator />
      </Select.Trigger>
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
              // A disabled option with a reason is not faded as a whole: the name and icon are, and the
              // reason keeps the secondary text colour.
              className={option.isDisabled && option.reason ? "pe-7 data-[disabled=true]:opacity-100" : "pe-7"}
              key={option.value}
              id={option.value}
              // Type-to-select matches on, and the hidden native option reads, "label, badge";
              // the option's own accessible name comes from its content, where the badge's hidden comma separates them.
              textValue={option.badge ? `${option.label}, ${option.badge}` : option.label}
              isDisabled={option.isDisabled}
            >
              {option.icon && (
                <span aria-hidden="true" className={`flex shrink-0 ${option.isDisabled && option.reason ? FADED : ""}`}>
                  {option.icon}
                </span>
              )}
              {option.detail ? (
                <span className={`flex min-w-0 flex-col ${option.badge ? "" : "flex-1"}`}>
                  <FadeOverflow lang={option.lang} dir="auto" className={option.isDisabled && option.reason ? `min-w-0 ${FADED}` : "min-w-0"} titleWhenClipped={option.label}>
                    {option.label}
                  </FadeOverflow>
                  <Description className="sr-only">{option.detail}</Description>
                  {option.reason && <span aria-hidden="true" className="text-xs text-app-muted"><ReasonText reason={option.reason} /></span>}
                </span>
              ) : (
                <FadeOverflow lang={option.lang} dir="auto" className={option.badge ? "min-w-0" : "min-w-0 flex-1"} titleWhenClipped={option.label}>
                  {option.label}
                </FadeOverflow>
              )}
              {option.badge && <StatusBadge>{option.badge}</StatusBadge>}
              <ListBox.ItemIndicator />
            </ListBox.Item>
          ))}
        </MotionListBox>
      </Select.Popover>
      {description && <Description id={descriptionId}>{description}</Description>}
      {descriptionDetails}
    </Select>
  );
}
