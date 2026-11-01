// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { CloneDirSetting } from "./CloneDirSetting";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn((_body?: { type: string }) => Promise.resolve({}) as Promise<never>);

vi.mock("../store", () => ({
  useDaemon: () => ({ request, raiseError: () => {} }),
  useDaemonStore: (selector: (state: { settings: { default_clone_dir: string }; homeDir: string }) => unknown) =>
    selector({ settings: { default_clone_dir: "/home/dev/Projects" }, homeDir: "/home/dev" }),
}));

/** Mounts the field and returns its input, with the keys that bubbled past React's own listener on
 * the container to `<body>`, where a dialog portalled there would be waiting for Escape. */
function mount() {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const bubbled: string[] = [];
  const record = (event: KeyboardEvent) => bubbled.push(event.key);
  document.body.addEventListener("keydown", record);
  onTestFinished(() => {
    act(() => root.unmount());
    document.body.removeEventListener("keydown", record);
    container.remove();
  });
  act(() => root.render(<CloneDirSetting />));
  const input = container.querySelector("input")!;
  const press = (key: string) =>
    act(() => {
      input.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
    });
  return {
    input,
    bubbled,
    press,
  };
}

it("leaves Escape to whatever surrounds the field", () => {
  const { bubbled, press } = mount();
  press("Escape");
  expect(bubbled).toEqual(["Escape"]);
});

it("sends what is typed on Enter", async () => {
  request.mockClear();
  const { input, press } = mount();
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "/srv/repos");
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  press("Enter");
  expect(request).toHaveBeenCalledWith({ type: "update_settings", default_clone_dir: "/srv/repos" });
  // The field goes back to the daemon's value once the send is answered.
  await act(async () => {});
});

it("does not send on the Enter that confirms a composition", () => {
  request.mockClear();
  const { input } = mount();
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "/srv/repos");
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  act(() => {
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", isComposing: true, bubbles: true }));
  });
  expect(request).not.toHaveBeenCalled();
});

const browse = () => Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === "Browse")!;

it("opens the directory browser at the stored directory", async () => {
  request.mockClear();
  mount();
  await act(async () => browse().click());
  expect(request).toHaveBeenCalledWith({ type: "list_dir", path: "/home/dev/Projects" });
});

it("sends the directory picked in the browser once", async () => {
  request.mockClear();
  request.mockImplementation((body) => Promise.resolve(body?.type === "list_dir" ? { type: "dir_listing", path: "/srv/new", entries: [] } : {}) as Promise<never>);
  mount();
  await act(async () => browse().click());
  await act(async () => Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === "Select this directory")!.click());
  expect(request.mock.calls.filter(([body]) => body?.type === "update_settings")).toEqual([[{ type: "update_settings", default_clone_dir: "/srv/new" }]]);
  expect(document.body.querySelector('input[aria-label="Directory path"]')).toBeNull();
});

// jsdom does not move focus on a mouse press, so the move the browser would make is made by hand: the press has to put
// focus back in the field before anything sees the field lose it, which would send what is typed.
it("keeps a mouse press on Browse from taking focus from the field", () => {
  request.mockClear();
  const { input } = mount();
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "/srv/repos");
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.focus();
  });
  act(() => {
    browse().dispatchEvent(new MouseEvent("mousedown", { button: 0, bubbles: true, cancelable: true }));
    browse().focus();
  });
  expect(document.activeElement).toBe(input);
  expect(request).not.toHaveBeenCalled();
});

// The picker taking focus is not the user leaving the field: what is typed is sent only once focus leaves the field
// again after a cancelled picker has handed it back.
it("does not send what is typed when focus moves into the picker, and sends it on the field's next blur", async () => {
  request.mockClear();
  request.mockImplementation((body) => Promise.resolve(body?.type === "list_dir" ? { type: "dir_listing", path: "/home/dev/Projects", entries: [] } : {}) as Promise<never>);
  const { input } = mount();
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "/srv/repos");
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.focus();
  });
  await act(async () => browse().click());
  const pickerInput = document.body.querySelector<HTMLInputElement>('input[aria-label="Directory path"]')!;
  act(() => pickerInput.focus());
  const sent = () => request.mock.calls.filter(([body]) => body?.type === "update_settings");
  expect(sent()).toEqual([]);
  await act(async () => Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === "Cancel")!.click());
  expect(document.body.querySelector('input[aria-label="Directory path"]')).toBeNull();
  act(() => input.focus());
  act(() => input.blur());
  expect(sent()).toEqual([[{ type: "update_settings", default_clone_dir: "/srv/repos" }]]);
});
