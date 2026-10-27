import { Description, Label } from "@heroui/react";
import React, { useId } from "react";

import { CodeToneContext } from "../components/codeTone";

/** A label over a group of controls that read as one field, visible unless `hideLabel`, named for assistive technology
 * as the group, with an optional `description` under them that the group is described by and an optional `help` tip at
 * the end of the label's line, beside the label rather than inside it so it is not part of the group's accessible name.
 * HeroUI's `Fieldset` is not used: its legend is drawn a size up from `Label` and flush against what follows it, and a
 * native `<fieldset>` lays its legend out by rules of its own rather than as a flex item, so matching the labels of the
 * fields beside it would take an override on the root and on the legend alike — more than the `div`, `Label` and
 * `Description` this assembles. HeroUI dims a field's label through `[data-disabled=true] .label`, which a label
 * outside the controls never sits under, so `isDisabled` marks the group `div` for it. With `errorMessage` the group is
 * the field the message is about: it is drawn under the controls as a `FieldError` is, with the `errorId` its controls
 * point to as their description, so it is heard where focus lands and once: the group itself is not described by it,
 * and loses its description while the message shows, as HeroUI hides a field's description while it is invalid. */
export function LabelledGroup({
  label,
  hideLabel,
  help,
  description,
  isDisabled,
  errorMessage,
  errorId,
  children,
}: {
  label: string;
  /** Leaves the label off the screen, for a group that what holds it already names (a tab); it still names the
   * group. */
  hideLabel?: boolean;
  help?: React.ReactNode;
  description?: React.ReactNode;
  isDisabled?: boolean;
  errorMessage?: React.ReactNode;
  /** The id the message carries, for the controls it describes. */
  errorId?: string;
  children: React.ReactNode;
}): React.ReactElement {
  const labelId = useId();
  const descriptionId = useId();
  return (
    <div
      role="group"
      aria-labelledby={labelId}
      aria-describedby={description && !errorMessage ? descriptionId : undefined}
      data-disabled={isDisabled || undefined}
      className="flex flex-col gap-1.5"
    >
      {help ? (
        <div className="flex items-center gap-2">
          <Label id={labelId} className={hideLabel ? "sr-only" : undefined}>{label}</Label>
          {help}
        </div>
      ) : (
        <Label id={labelId} className={hideLabel ? "sr-only" : undefined}>{label}</Label>
      )}
      {children}
      {description && !errorMessage && <Description id={descriptionId}>{description}</Description>}
      {errorMessage && (
        // Drawn as a field's `FieldError` is (a group is no field to hold one), and announced as a field's is.
        <div id={errorId} data-slot="field-error" className="px-1 text-xs wrap-break-word text-danger">
          <CodeToneContext.Provider value="danger"><span role="alert">{errorMessage}</span></CodeToneContext.Provider>
        </div>
      )}
    </div>
  );
}
