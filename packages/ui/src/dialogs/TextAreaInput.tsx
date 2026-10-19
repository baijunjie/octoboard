import { FieldError, Label, TextArea, TextField } from "@heroui/react";
import React from "react";

import { CodeToneContext } from "../components/codeTone";
import { TextFieldDescription, TextLengthHint } from "./TextFieldDescription";

/** A labelled multi-line text field, in HeroUI's variant for a field on a surface, for free text
 * the user writes at length. It does not trim what is typed, so leading and trailing whitespace
 * survive while editing and the form decides what to do with them, and Enter adds a line rather
 * than submitting the dialog. The placeholder names what an empty field means; `description`
 * carries anything else the field has to say. */
export function TextAreaInput({
  label,
  value,
  onChange,
  placeholder,
  description,
  isDisabled,
  rows = 6,
  errorMessage,
  characterLimit,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  description?: string;
  isDisabled?: boolean;
  rows?: number;
  errorMessage?: string;
  characterLimit?: number;
}): React.ReactElement {
  const invalid = !!errorMessage;
  return (
    <TextField fullWidth variant="secondary" value={value} onChange={onChange} isDisabled={isDisabled} isInvalid={invalid} validationBehavior="aria">
      <Label>{label}</Label>
      <TextArea rows={rows} placeholder={placeholder} />
      <TextFieldDescription value={value} characterLimit={characterLimit} description={description} />
      <FieldError>
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
