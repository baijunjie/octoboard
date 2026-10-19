import { FieldError, Input, Label, TextField } from "@heroui/react";
import React from "react";

import { CodeToneContext } from "../components/codeTone";
import { useTrimmedField } from "./useTrimmedField";
import { TextFieldDescription, TextLengthHint } from "./TextFieldDescription";

/** A labelled single-line text field, in HeroUI's variant for a field on a surface, since it is
 * always in a dialog. `ltr` lays out a value that always reads left to right (a
 * path, a URL) whatever the UI's direction. The placeholder is an example; `description` carries
 * anything else the field has to say, and `errorMessage` marks the field invalid and says what is
 * wrong with it, right under the field — HeroUI hides the description for as long as that lasts.
 * `trailing` is a control that belongs to the field, such as a browse button, and sits beside the
 * input. */
export function TextInput({
  label,
  value,
  onChange,
  placeholder,
  description,
  errorMessage,
  trailing,
  autoFocus,
  dir,
  isDisabled,
  characterLimit,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  description?: string;
  errorMessage?: React.ReactNode;
  trailing?: React.ReactNode;
  autoFocus?: boolean;
  dir?: "ltr";
  isDisabled?: boolean;
  characterLimit?: number;
}): React.ReactElement {
  const field = useTrimmedField(value, onChange);
  const invalid = errorMessage !== undefined && errorMessage !== null && errorMessage !== false && errorMessage !== "";
  // Dialog fields hold application metadata, so 1Password must not offer identity or login fills.
  const input = <Input data-1p-ignore dir={dir} placeholder={placeholder} className={trailing ? "min-w-0 flex-1" : undefined} />;
  return (
    <TextField
      fullWidth
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
      <Label>{label}</Label>
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
