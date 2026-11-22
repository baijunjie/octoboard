// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { fakeTimers } from "../../test/clock";
import { settleFrames } from "../dialog/dialogTestSupport";
import { useFocusReturnAfterClose } from "./useFocusReturnAfterClose";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function ClosesOnPress({ restore }: { restore: () => void }): React.ReactElement {
  const handBack = useFocusReturnAfterClose(restore);
  return <button onClick={handBack}>close</button>;
}

/** Mounts the surface whose button stands for the dialog's close, and takes it down when the test finishes. */
function mount(restore: () => void): { button: () => HTMLButtonElement; unmount: () => void } {
  fakeTimers();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  let mounted = true;
  const unmount = () => {
    if (!mounted) return;
    mounted = false;
    act(() => root.unmount());
  };
  onTestFinished(() => {
    unmount();
    container.remove();
  });
  act(() => root.render(<ClosesOnPress restore={restore} />));
  return { button: () => container.querySelector("button")!, unmount };
}

const press = (element: HTMLElement) => act(() => element.click());

it("hands focus on two frames after the dialog closed, only if it fell to the page", async () => {
  const restore = vi.fn();
  const { button } = mount(restore);
  press(button());
  (document.activeElement as HTMLElement | null)?.blur();
  await settleFrames(2);
  expect(restore).toHaveBeenCalledOnce();

  // Focus that is somewhere already is left alone.
  const elsewhere = document.body.appendChild(document.createElement("input"));
  onTestFinished(() => elsewhere.remove());
  press(button());
  elsewhere.focus();
  await settleFrames(2);
  expect(restore).toHaveBeenCalledOnce();
});

it("hands focus on to nothing once the surface that opened the dialog is gone", async () => {
  const restore = vi.fn();
  const { button, unmount } = mount(restore);
  press(button());
  unmount();
  await settleFrames(2);
  expect(restore).not.toHaveBeenCalled();
});
