// @vitest-environment jsdom
import { beforeEach, expect, it, vi } from "vitest";

import { handFocusOff } from "./handFocusOff";

const setInteractionModality = vi.hoisted(() => vi.fn());
vi.mock("react-aria", () => ({ setInteractionModality }));

let from: HTMLElement;
let holder: HTMLElement;

beforeEach(() => {
  setInteractionModality.mockClear();
  document.body.innerHTML = "";
  from = document.body.appendChild(document.createElement("div"));
  from.appendChild(document.createElement("button")).textContent = "leaving";
  holder = document.body.appendChild(document.createElement("div"));
  holder.appendChild(document.createElement("button")).textContent = "destination";
});

const destination = () => holder.querySelector("button");

it("hands focus to the holder's button when `from` holds it", () => {
  from.querySelector("button")?.focus();
  handFocusOff(from, holder, true);
  expect(document.activeElement).toBe(destination());
  expect(setInteractionModality).toHaveBeenCalledWith("keyboard");
});

it("leaves focus alone when it is not inside `from`", () => {
  const outside = document.body.appendChild(document.createElement("button"));
  outside.focus();
  handFocusOff(from, holder, true);
  expect(document.activeElement).toBe(outside);
  expect(setInteractionModality).not.toHaveBeenCalled();
});

it("does nothing when the caller says nothing is going away", () => {
  from.querySelector("button")?.focus();
  const focused = document.activeElement;
  handFocusOff(null, holder, true);
  expect(document.activeElement).toBe(focused);
  expect(setInteractionModality).not.toHaveBeenCalled();
});

it("leaves the interaction modality as the press set it without `showRing`", () => {
  from.querySelector("button")?.focus();
  handFocusOff(from, holder, false);
  expect(document.activeElement).toBe(destination());
  expect(setInteractionModality).not.toHaveBeenCalled();
});
