import { Description as HeroDescription, FieldError as HeroFieldError, Label as HeroLabel } from "@heroui/react";
import React, { createContext, useContext } from "react";
import { ButtonContext } from "react-aria-components";

import { definedProps } from "./props";

/** HeroUI's field variants: `primary` for a field on a pane's or the page's own background, `secondary` for one on a
 * surface (a dialog, a popover, a menu, a card). */
export type FieldVariant = "primary" | "secondary";

/** Whether the field around an input was given `autoFocus`, for the input to carry the opening-focus mark
 * (`openingFocus`): the field takes the prop, its input is the element focused. */
export const FieldAutoFocusContext = createContext(false);

export function useFieldAutoFocus(): boolean {
  return useContext(FieldAutoFocusContext);
}

/** The look of a field that is shown but may not be changed (a report page that is locked): a value to read, not a
 * control to use. The fill and the shadow that mark a control go, a plain 1px border stays, and the text keeps its full
 * strength where HeroUI would dim it as disabled. */
export const LOCKED_FIELD_CLASS = "border border-separator bg-transparent shadow-none data-[disabled=true]:opacity-100";

/** Whether the field around an input is `locked`, for its input to take the locked look. */
export const FieldLockedContext = createContext(false);

export function useFieldLocked(): boolean {
  return useContext(FieldLockedContext);
}

/** Content inside a field that is not one of the field's own parts (a status mark): a field hands its own button's
 * behaviour to every button inside it, so it is reset around such content, which keeps its buttons their own. */
export function OutsideFieldParts({ children }: { children: React.ReactNode }): React.ReactElement {
  return <ButtonContext.Provider value={{}}>{children}</ButtonContext.Provider>;
}

/** HeroUI's label: a field's visible name, which names the field it sits in, or, given an `id`, whatever points to it.
 * Inside a menu or list item it is the item's name. */
export function Label({ id, dir, className, children }: {
  id?: string;
  dir?: "auto" | "ltr";
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return <HeroLabel {...definedProps({ id, dir, className })}>{children}</HeroLabel>;
}

/** Props that make the element they go on, a group of controls around a `Label` that sits outside the controls, read as
 * disabled while `disabled`: HeroUI dims a field's label through `[data-disabled=true] .label`, a hook the label of a
 * group never sits under by itself. */
export function disabledGroupProps(disabled: boolean | undefined): { "data-disabled"?: true } {
  return disabled ? { "data-disabled": true } : {};
}

/** HeroUI's description: the note under a field, which describes the field it sits in, or, given an `id`, whatever
 * points to it. Inside a menu or list item it is the item's description. */
export function Description({ id, dir, className, children }: {
  id?: string;
  dir?: "auto";
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return <HeroDescription {...definedProps({ id, dir, className })}>{children}</HeroDescription>;
}

/** HeroUI's field error: what is wrong with the field it sits in, shown only while the field is invalid. */
export function FieldError({ children }: { children: React.ReactNode }): React.ReactElement {
  return <HeroFieldError>{children}</HeroFieldError>;
}

/** The error message of a group of fields, which holds no `FieldError` of its own as it is no field: drawn and marked as
 * a field's error is. It is shown whenever it is rendered; whatever announces it goes in `children`. */
export function GroupFieldError({ id, children }: { id?: string; children: React.ReactNode }): React.ReactElement {
  return <div id={id} data-slot="field-error" className="px-1 text-xs wrap-break-word text-danger">{children}</div>;
}

/** What a field gives the clear button placed in it (`SearchField`'s and `SelectClearButton`'s `children`): the
 * props that make the layer's `Button` the field's clear action, and, in `className`, where the field seats it. */
export type FieldClearProps = Record<string, unknown> & { className?: string };

export const FieldClearContext = createContext<FieldClearProps | null>(null);

export function useFieldClear(): FieldClearProps | null {
  return useContext(FieldClearContext);
}
