import { FieldError, Input, InputGroup, Label, TextField } from "@heroui/react";
import React from "react";

import { CodeToneContext } from "../components/codeTone";
import { openingFocus } from "../components/regionFocus";
import { useTrimmedField } from "./useTrimmedField";
import { TextFieldDescription, TextLengthHint } from "./TextFieldDescription";

/** A labelled single-line text field, in HeroUI's variant for a field on a surface, since it is
 * always in a dialog. `ltr` lays out a value that always reads left to right (a
 * path, a URL) whatever the UI's direction. The placeholder is a required example or optional default; `description` carries
 * anything else the field has to say, and `errorMessage` marks the field invalid and says what is
 * wrong with it, right under the field — HeroUI hides the description for as long as that lasts.
 * `trailing` is a control that belongs to the field, such as a browse button, and sits beside the
 * input. `endAdornment` is a decoration inside the field's frame at its end (a spinner), seated by
 * HeroUI's input group; it is not interactive. The field keeps one structure for as long as
 * `endAdornment` is given at all (`null` for "nothing at the moment"), so an adornment that comes
 * and goes while the user types does not remount the input and drop its focus. */
export function TextInput({
  label,
  labelContent,
  value,
  onChange,
  placeholder,
  description,
  describedBy,
  errorMessage,
  trailing,
  endAdornment,
  autoFocus,
  dir,
  isDisabled,
  characterLimit,
}: {
  label: string;
  /** A visible label that shows less than the field's name ("Directory" for "Directory for Notes"): the
   * input is named by `label` alone. `label` must contain the visible text verbatim, so the label stays
   * in the name (WCAG 2.5.3), and this is hidden from assistive technology, so it holds nothing interactive. */
  labelContent?: React.ReactNode;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  description?: React.ReactNode;
  /** The id of something outside the field that describes it, such as the Alert under it holding the
   * failure of a lookup the field makes; read after the field's own description and error. */
  describedBy?: string;
  errorMessage?: React.ReactNode;
  trailing?: React.ReactNode;
  endAdornment?: React.ReactNode;
  autoFocus?: boolean;
  dir?: "ltr";
  isDisabled?: boolean;
  characterLimit?: number;
}): React.ReactElement {
  const field = useTrimmedField(value, onChange);
  const invalid = errorMessage !== undefined && errorMessage !== null && errorMessage !== false && errorMessage !== "";
  // Dialog fields hold application metadata, so 1Password must not offer identity or login fills.
  const input = endAdornment !== undefined ? (
    // The direction is the group's, so the affix stays on the input's side whatever the UI's direction.
    <InputGroup variant="secondary" fullWidth dir={dir} className={trailing ? "min-w-0 flex-1" : undefined}>
      <InputGroup.Input data-1p-ignore {...openingFocus(autoFocus)} placeholder={placeholder} />
      <InputGroup.Suffix>{endAdornment}</InputGroup.Suffix>
    </InputGroup>
  ) : (
    <Input data-1p-ignore {...openingFocus(autoFocus)} dir={dir} placeholder={placeholder} className={trailing ? "min-w-0 flex-1" : undefined} />
  );
  return (
    <TextField
      fullWidth
      aria-describedby={describedBy}
      variant="secondary"
      {...field}
      autoFocus={autoFocus}
      isDisabled={isDisabled}
      isInvalid={invalid}
      // Whether to submit is the dialog's own decision: react-aria's default behaviour hands a
      // field marked invalid to the browser's constraint validation, which then refuses the submit
      // before the dialog's handler — and the mark itself only ever appears from that handler.
      validationBehavior="aria"
    >
      {/* The visible text is hidden from the name and the plain label stands in for it, so the name
          is exactly `label` and not the two put together (react-aria names the input by the label
          element). The name is not selectable, so copying across the label takes only what is drawn. */}
      {labelContent ? (
        <Label>
          <span aria-hidden="true">{labelContent}</span>
          <span className="sr-only select-none">{label}</span>
        </Label>
      ) : (
        <Label>{label}</Label>
      )}
      {/* The row keeps the trailing control level with the input while the description and the
          error stay under both: HeroUI's text field is a plain flex column whose styling reaches
          its input by descendant selector, so a row of its own around them changes nothing.
          HeroUI's own `InputGroup` would seat the control inside the field's frame, which is a
          different control from the secondary button the project dialog browses with. */}
      {trailing ? (
        <div className="flex items-center gap-2">
          {input}
          {trailing}
        </div>
      ) : (
        input
      )}
      <TextFieldDescription value={value} characterLimit={characterLimit} description={description} />
      {/* Renders nothing while the field is valid. The alert is on an element of its own because
          react-aria drops a `role` given to the error itself: the message appears on a submit that
          gives the user no other feedback, so it has to be announced as a dialog-wide failure is. */}
      <FieldError>
        {/* The error is danger-toned text, so a literal inside it is too, as in a danger alert. */}
        <CodeToneContext.Provider value="danger">
          <span role="alert">
            {invalid ? errorMessage : null}
            {invalid && characterLimit !== undefined && <TextLengthHint value={value} limit={characterLimit} />}
          </span>
        </CodeToneContext.Provider>
      </FieldError>
    </TextField>
  );
}
