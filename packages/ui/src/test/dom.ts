// Queries and input helpers over the document body, shared by the dialog test files.
import { act } from "react";

export const buttons = () => Array.from(document.body.querySelectorAll<HTMLButtonElement>("button"));
export const optionElements = () => Array.from(document.body.querySelectorAll<HTMLElement>('[role="option"]'));
export const click = (el: Element | null | undefined) => act(async () => (el as HTMLElement | undefined)?.click());

/** Sets `el`'s value the way the platform does, so React sees the input event, and fires it. */
export function type(el: HTMLInputElement | HTMLTextAreaElement, value: string) {
  act(() => {
    Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), "value")!.set!.call(el, value);
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

/** The input under `scope` that the label `label` names. */
export const inputLabelled = (label: string, scope: ParentNode = document.body) =>
  Array.from(scope.querySelectorAll<HTMLInputElement>("input")).find((el) => el.labels?.[0]?.textContent === label)!;

/** `text` without the bidi isolates a name substituted into a sentence carries (see `isolate`). */
export const withoutIsolates = (text: string | null | undefined) => text?.replace(/[\u2068\u2069]/g, "");

/** The picker trigger named `name`, by its own label or by the element it is labelled by, with a substituted name's
 * bidi isolates left out. */
export const pickerNamed = (name: string) => buttons().find((button) =>
  withoutIsolates(button.getAttribute("aria-label")) === name ||
  button.getAttribute("aria-labelledby")?.split(" ").some((id) => withoutIsolates(document.getElementById(id)?.textContent) === name),
)!;
