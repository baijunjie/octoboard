// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { installPreviewKeyboardGuard } from "./keyboard";

let remove = () => {};
afterEach(() => {
  remove();
  document.body.replaceChildren();
  vi.unstubAllGlobals();
});

function pointer(coarse: boolean) {
  vi.stubGlobal("matchMedia", vi.fn(() => ({ matches: coarse })));
}

it("protects a newly mounted terminal before synchronous focus and later dialog fields", async () => {
  pointer(true);
  remove = installPreviewKeyboardGuard();
  const terminal = document.createElement("textarea");
  document.body.append(terminal);
  terminal.focus();
  expect(terminal.readOnly).toBe(true);
  expect(terminal.inputMode).toBe("none");
  expect(document.activeElement).toBe(terminal);

  const dialog = document.createElement("div");
  dialog.innerHTML = '<input type="search"><input type="checkbox"><button>Browse</button>';
  document.body.append(dialog);
  await Promise.resolve();
  expect(dialog.querySelector("input")!.readOnly).toBe(true);
  const checkbox = dialog.querySelector<HTMLInputElement>('[type="checkbox"]')!;
  expect(checkbox.readOnly).toBe(false);
  checkbox.click();
  expect(checkbox.checked).toBe(true);
  dialog.querySelector("button")!.focus();
  expect(document.activeElement).toBe(dialog.querySelector("button"));
});

it("keeps desktop preview inputs editable", async () => {
  pointer(false);
  remove = installPreviewKeyboardGuard();
  const field = document.createElement("textarea");
  document.body.append(field);
  field.focus();
  await Promise.resolve();
  expect(window.matchMedia).toHaveBeenCalledWith("(pointer: coarse)");
  expect(field.readOnly).toBe(false);
  expect(field.inputMode).toBe("");
});

it("removes observation and focus handling on disposal", async () => {
  pointer(true);
  remove = installPreviewKeyboardGuard();
  remove();
  const field = document.createElement("textarea");
  document.body.append(field);
  field.focus();
  await Promise.resolve();
  expect(field.readOnly).toBe(false);
});
