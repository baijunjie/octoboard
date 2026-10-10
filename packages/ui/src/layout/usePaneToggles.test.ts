// @vitest-environment jsdom
import { afterEach, expect, it } from "vitest";

import { holdsText } from "./usePaneToggles";

afterEach(() => document.body.replaceChildren());

function field(html: string): Element {
  document.body.innerHTML = html;
  return document.body.firstElementChild!;
}

it("leaves Escape to a text field while it holds text, and to the pane once it is empty", () => {
  const input = field('<input type="search" value="abc">') as HTMLInputElement;
  expect(holdsText(input)).toBe(true);
  input.value = "";
  expect(holdsText(input)).toBe(false);
});

it("leaves Escape to the pane for xterm's hidden textarea, even with text in it", () => {
  expect(holdsText(field('<textarea class="xterm-helper-textarea">x</textarea>'))).toBe(false);
});

it.each([
  ['<input type="checkbox">', "a checkbox, whose value is always 'on'"],
  ['<input type="range" value="5">', "a range"],
  ['<input type="text" value="abc" readonly>', "a read-only field"],
])("leaves Escape to the pane for %s (%s)", (html) => {
  expect(holdsText(field(html))).toBe(false);
});
