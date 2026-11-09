// @vitest-environment jsdom
// Loaded first, as `main.tsx` loads it, so every listener below is registered after the guard's.
import "./focusGuard";
import { expect, it, onTestFinished, vi } from "vitest";
import { afterQueuedWork } from "../test/clock";

// Past the frame of a press and the tick after it, where the guard disarms.
const afterGuard = () => new Promise((resolve) => requestAnimationFrame(() => void afterQueuedWork().then(resolve)));

function hasFocus(value: boolean) {
  const spy = vi.spyOn(document, "hasFocus").mockReturnValue(value);
  onTestFinished(() => spy.mockRestore());
}

it("lets the window's own focus and blur reach the listeners registered after it", () => {
  const heard = vi.fn();
  window.addEventListener("focus", heard);
  window.addEventListener("blur", heard);
  window.dispatchEvent(new FocusEvent("blur"));
  window.dispatchEvent(new FocusEvent("focus"));
  window.removeEventListener("focus", heard);
  window.removeEventListener("blur", heard);
  expect(heard).toHaveBeenCalledTimes(2);
});

it("swallows window-targeted focus events from a press's mousedown until the tick after the next frame, and no others", async () => {
  // jsdom has no focus of its own to report, which the guard would replay as a change.
  hasFocus(false);
  window.dispatchEvent(new FocusEvent("blur"));
  const onWindow = vi.fn();
  const onElement = vi.fn();
  const button = document.body.appendChild(document.createElement("button"));
  for (const type of ["focus", "blur", "focusin", "focusout"]) window.addEventListener(type, onWindow, true);
  button.addEventListener("focusout", onElement);
  button.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
  for (const type of ["focus", "blur", "focusin", "focusout"]) window.dispatchEvent(new FocusEvent(type));
  button.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
  expect(onWindow).toHaveBeenCalledTimes(1);
  expect(onWindow.mock.calls[0]![0].target).toBe(button);
  expect(onElement).toHaveBeenCalledTimes(1);
  await afterGuard();
  window.dispatchEvent(new FocusEvent("blur"));
  expect(onWindow).toHaveBeenCalledTimes(2);
  for (const type of ["focus", "blur", "focusin", "focusout"]) window.removeEventListener(type, onWindow, true);
  button.remove();
});

function press() {
  document.body.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
}

it("replays a blur swallowed while armed once the guard is off, when the window no longer has focus", async () => {
  hasFocus(true);
  window.dispatchEvent(new FocusEvent("focus"));
  const heard = vi.fn();
  window.addEventListener("blur", heard);
  onTestFinished(() => window.removeEventListener("blur", heard));
  press();
  hasFocus(false);
  window.dispatchEvent(new FocusEvent("blur"));
  expect(heard).not.toHaveBeenCalled();
  await afterGuard();
  expect(heard).toHaveBeenCalledTimes(1);
});

it("replays a focus swallowed while armed, as a click that activates the window gives", async () => {
  hasFocus(false);
  window.dispatchEvent(new FocusEvent("blur"));
  const heard = vi.fn();
  window.addEventListener("focus", heard);
  onTestFinished(() => window.removeEventListener("focus", heard));
  press();
  hasFocus(true);
  window.dispatchEvent(new FocusEvent("focus"));
  expect(heard).not.toHaveBeenCalled();
  await afterGuard();
  expect(heard).toHaveBeenCalledTimes(1);
});

it("replays nothing when the window's focus is what the last event let through said", async () => {
  hasFocus(true);
  window.dispatchEvent(new FocusEvent("focus"));
  const heard = vi.fn();
  window.addEventListener("focus", heard);
  window.addEventListener("blur", heard);
  onTestFinished(() => {
    window.removeEventListener("focus", heard);
    window.removeEventListener("blur", heard);
  });
  press();
  window.dispatchEvent(new FocusEvent("blur"));
  window.dispatchEvent(new FocusEvent("focus"));
  await afterGuard();
  expect(heard).not.toHaveBeenCalled();
});

it("stays armed through a second press that comes before it disarms", async () => {
  hasFocus(true);
  window.dispatchEvent(new FocusEvent("focus"));
  const heard = vi.fn();
  window.addEventListener("blur", heard);
  onTestFinished(() => window.removeEventListener("blur", heard));
  press();
  await new Promise((resolve) => requestAnimationFrame(resolve));
  press();
  await afterQueuedWork();
  window.dispatchEvent(new FocusEvent("blur"));
  expect(heard).not.toHaveBeenCalled();
  await afterGuard();
  window.dispatchEvent(new FocusEvent("blur"));
  expect(heard).toHaveBeenCalledTimes(1);
});
