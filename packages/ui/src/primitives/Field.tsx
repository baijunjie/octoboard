import { Description as HeroDescription, FieldError as HeroFieldError, Label as HeroLabel } from "@heroui/react";
import React, { createContext, useContext } from "react";
import { ButtonContext } from "react-aria-components";

import { definedProps } from "./props";

/** HeroUI's field variants: `primary` for a field on the page's background, `secondary` for one on a surface (a dialog,
 * a settings page, a pane's toolbar). */
export type FieldVariant = "primary" | "secondary";

/** Whether the field around an input was given `autoFocus`, for the input to carry the opening-focus mark
 * (`openingFocus`): the field takes the prop, its input is the element focused. */
export const FieldAutoFocusContext = createContext(false);

export function useFieldAutoFocus(): boolean {
  return useContext(FieldAutoFocusContext);
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

/** HeroUI's description: the note under a field, which describes the field it sits in, or, given an `id`, whatever
 * points to it. Inside a menu or list item it is the item's description. */
export function Description({ id, className, children }: {
  id?: string;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return <HeroDescription {...definedProps({ id, className })}>{children}</HeroDescription>;
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
