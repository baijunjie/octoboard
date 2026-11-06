import {
  Input as HeroInput,
  InputGroup as HeroInputGroup,
  SearchField as HeroSearchField,
  TextArea as HeroTextArea,
  TextField as HeroTextField,
} from "@heroui/react";
import React from "react";

import { useDisabled } from "./busy";
import { FieldAutoFocusContext, FieldClearContext, type FieldVariant, OutsideFieldParts, useFieldAutoFocus } from "./Field";
import { openingFocus } from "./focus";
import { type DataAttributes, definedProps } from "./props";

/** What a text field and a search field share: the value, how the field is named and described, and its state. */
interface TextFieldBaseProps {
  value: string;
  onChange: (value: string) => void;
  onBlur?: () => void;
  /** The accessible name, for a field with no visible `Label`. */
  label?: string;
  /** The ids of further elements that describe the field, read after its own description and error. */
  describedBy?: string;
  disabled?: boolean;
  /** Focuses the field's input as it mounts, and marks it as where its dialog's opening focus goes (`openingFocus`). */
  autoFocus?: boolean;
  variant?: FieldVariant;
  fullWidth?: boolean;
  className?: string;
  children: React.ReactNode;
}

/** HeroUI's text field: the field around a `Label`, an `Input` or a `TextArea`, a `Description` and a `FieldError`,
 * which ties them together. With `invalid` it is marked invalid and its `FieldError` shows. Whether a form goes ahead
 * is its own decision: an invalid field never hands the submit to the browser's constraint validation, which would
 * refuse it before the form's handler — and the mark itself only ever appears from that handler. */
export function TextField({ value, onChange, onBlur, label, describedBy, disabled, invalid, autoFocus, variant, fullWidth, className, children }: TextFieldBaseProps & {
  invalid?: boolean;
}): React.ReactElement {
  return (
    <HeroTextField
      value={value}
      onChange={onChange}
      onBlur={onBlur && (() => onBlur())}
      aria-label={label}
      aria-describedby={describedBy}
      isDisabled={useDisabled(disabled)}
      isInvalid={invalid}
      validationBehavior="aria"
      autoFocus={autoFocus}
      variant={variant}
      fullWidth={fullWidth}
      className={className}
    >
      <FieldAutoFocusContext.Provider value={!!autoFocus}>{children}</FieldAutoFocusContext.Provider>
    </HeroTextField>
  );
}

/** HeroUI's input. Inside a `TextField` it is the field's input and takes its value from the field; on its own it
 * holds `value`, named by `label`. With `start` or `end` it is HeroUI's input group, the adornment seated inside its
 * frame at that edge; `dir`, `className` and `onHoverChange` are then the group's, and `inputClassName` the input's.
 * The structure follows whether an adornment is given at all (`null` for "nothing at the moment"), so one that comes
 * and goes while the user types does not remount the input and drop its focus. */
export function Input({
  ref, value, onChange, onBlur, onKeyDown, label, describedBy, invalid, placeholder, dir, variant, fullWidth,
  autoComplete, autoCapitalize, autoCorrect, spellCheck, start, end, onHoverChange, className, inputClassName, ...data
}: DataAttributes & {
  ref?: React.Ref<HTMLInputElement>;
  value?: string;
  onChange?: (value: string) => void;
  onBlur?: () => void;
  onKeyDown?: (event: React.KeyboardEvent<HTMLInputElement>) => void;
  label?: string;
  describedBy?: string;
  /** Marks an input on its own invalid (`aria-invalid`); inside a field, the field's `invalid` does. */
  invalid?: boolean;
  placeholder?: string;
  dir?: "auto" | "ltr";
  variant?: FieldVariant;
  fullWidth?: boolean;
  autoComplete?: "off";
  autoCapitalize?: "off";
  autoCorrect?: "off";
  spellCheck?: boolean;
  start?: React.ReactNode;
  end?: React.ReactNode;
  onHoverChange?: (hovered: boolean) => void;
  className?: string;
  inputClassName?: string;
}): React.ReactElement {
  const fieldAutoFocus = useFieldAutoFocus();
  // Inside a field the field's own `disabled` reaches the input; a busy modal disables it there too.
  const disabled = useDisabled();
  const adorned = start !== undefined || end !== undefined;
  const inputProps = {
    ...data,
    ...openingFocus(fieldAutoFocus),
    ...definedProps({
      value,
      onChange: onChange && ((event: React.ChangeEvent<HTMLInputElement>) => onChange(event.target.value)),
      onBlur: onBlur && (() => onBlur()),
      onKeyDown,
      "aria-label": label,
      "aria-describedby": describedBy,
      "aria-invalid": invalid || undefined,
      "data-invalid": invalid ? "true" : undefined,
      disabled,
      placeholder,
      autoComplete,
      autoCapitalize,
      autoCorrect,
      spellCheck,
    }),
  };
  if (!adorned) {
    return <HeroInput ref={ref} {...inputProps} {...definedProps({ dir, variant, fullWidth, className })} />;
  }
  return (
    <HeroInputGroup {...definedProps({ dir, variant, fullWidth, className, onHoverChange, isDisabled: disabled })}>
      {start !== undefined && <HeroInputGroup.Prefix>{start}</HeroInputGroup.Prefix>}
      <HeroInputGroup.Input ref={ref} {...inputProps} {...definedProps({ className: inputClassName })} />
      {end !== undefined && <HeroInputGroup.Suffix>{end}</HeroInputGroup.Suffix>}
    </HeroInputGroup>
  );
}

/** HeroUI's text area, the multi-line input of a `TextField`; Enter adds a line rather than submitting. */
export function TextArea({ rows, placeholder }: { rows?: number; placeholder?: string }): React.ReactElement {
  return <HeroTextArea {...openingFocus(useFieldAutoFocus())} {...definedProps({ rows, placeholder })} />;
}

/** HeroUI's search field: a search icon, the input, and at its end `trailing` (a status mark, kept apart from the
 * field's own clear behaviour) and `clearButton`, a layer `Button` that becomes the field's clear action and is
 * seated at the end of the field. The field clears itself on Escape and calls `onSubmit` on Enter. `inputRef` holds
 * its input, and `controls` names what the search filters (`aria-controls`). */
export function SearchField({ inputRef, placeholder, controls, onSubmit, trailing, clearButton, value, onChange, onBlur, label, describedBy, disabled, autoFocus, variant, fullWidth, className }: Omit<TextFieldBaseProps, "children"> & {
  inputRef?: React.Ref<HTMLInputElement>;
  placeholder?: string;
  controls?: string;
  onSubmit?: () => void;
  trailing?: React.ReactNode;
  clearButton?: React.ReactElement;
}): React.ReactElement {
  return (
    <HeroSearchField
      value={value}
      onChange={onChange}
      onBlur={onBlur && (() => onBlur())}
      onSubmit={onSubmit && (() => onSubmit())}
      aria-label={label}
      aria-describedby={describedBy}
      aria-controls={controls}
      isDisabled={useDisabled(disabled)}
      autoFocus={autoFocus}
      variant={variant}
      fullWidth={fullWidth}
      className={className}
    >
      <HeroSearchField.Group>
        <HeroSearchField.SearchIcon />
        <HeroSearchField.Input ref={inputRef} {...definedProps({ placeholder })} {...openingFocus(autoFocus)} />
        {trailing !== undefined && <OutsideFieldParts>{trailing}</OutsideFieldParts>}
        {clearButton && <FieldClearContext.Provider value={SEARCH_CLEAR}>{clearButton}</FieldClearContext.Provider>}
      </HeroSearchField.Group>
    </HeroSearchField>
  );
}

/** The slot of HeroUI's search field that clears it, the mark HeroUI's styles find its clear button by, and how the
 * button sits at the end of the field's group. */
const SEARCH_CLEAR = { slot: "clear", "data-slot": "search-field-clear-button", className: "me-2 shrink-0" };
