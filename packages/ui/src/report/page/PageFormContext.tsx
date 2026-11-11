import { createContext, useContext } from "react";

import type { Component } from "../../protocol";
import type { FieldIssue, FieldValue, FormValues } from "./formModel";

/** A submitter's `name: value`, which a submission carries. */
export interface Submitter {
  name: string;
  value: string;
}

/** A press of a submitter: which button it was (`key`, from `submitterKeys` or `choiceKey`) and its `name: value`, if it
 * has one. */
export interface Pressed {
  key: string;
  submitter?: Submitter;
}

/** What the components of a page read of the page's form and act on: the values the fields show, what is wrong with
 * them, whether the page may be edited and a submission is out, and how a submitter submits. */
export interface PageFormState {
  values: FormValues;
  setValue: (name: string, value: FieldValue) => void;
  issues: ReadonlyMap<string, FieldIssue>;
  /** The form cannot be edited or submitted: the page is a history page, is submitted, or its submission is out. */
  locked: boolean;
  /** A submission this window sent is out. */
  pending: boolean;
  /** The key of the button whose submission is out. */
  pressed: string | undefined;
  /** The key of each `submit` button (`submitterKeys`). */
  submitKeys: ReadonlyMap<Component, string>;
  /** What the page was submitted with, if it was, for the submitter pressed to show as the one. */
  submission: Readonly<Record<string, unknown>> | null;
  isShown: (component: Component) => boolean;
  /** Submits the form, with the pressed button's `name: value` added where it has one. */
  press: (pressed: Pressed) => void;
  /** Checks the field's value against its `pattern`, which is tested when the user leaves the field and when the form is
   * submitted, not as they type. */
  checkPattern: (name: string) => void;
}

export const PageFormContext = createContext<PageFormState | undefined>(undefined);

export function usePageForm(): PageFormState {
  const form = useContext(PageFormContext);
  if (!form) throw new Error("A page component is drawn outside a page's form");
  return form;
}
