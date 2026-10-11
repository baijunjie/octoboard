// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { CloneDirSetting } from "./CloneDirSetting";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn(() => Promise.resolve({}) as Promise<never>);

vi.mock("../store", () => ({
  useDaemon: () => ({ request, toastError: () => {} }),
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
