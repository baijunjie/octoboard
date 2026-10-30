// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";

import { errorAlerts } from "../errorAlerts";
import { useAfterErrorAlert } from "./useAfterErrorAlert";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => { act(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); }); });

function mountHook(): { after: ReturnType<typeof useAfterErrorAlert>; unmount: () => void } {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const out = {} as { after: ReturnType<typeof useAfterErrorAlert>; unmount: () => void };
  function Probe(): null {
    out.after = useAfterErrorAlert();
    return null;
  }
  act(() => root.render(<Probe />));
  out.unmount = () => { act(() => root.unmount()); container.remove(); };
  return out;
}

it("waits for every alert already queued ahead of it, and runs once", () => {
  const { after, unmount } = mountHook();
  const then = vi.fn();
  act(() => { errorAlerts.report({ message: "first" }); errorAlerts.report({ message: "second" }); });
  after(then);
  act(() => errorAlerts.acknowledge());
  expect(then).not.toHaveBeenCalled();
  act(() => errorAlerts.acknowledge());
  expect(then).toHaveBeenCalledTimes(1);
  act(() => { errorAlerts.report({ message: "later" }); errorAlerts.acknowledge(); });
  expect(then).toHaveBeenCalledTimes(1);
  unmount();
});

it("waits for the alert reported after it was asked, and not for one that never comes before then", () => {
  const { after, unmount } = mountHook();
  const then = vi.fn();
  after(then);
  expect(then).not.toHaveBeenCalled();
  act(() => errorAlerts.report({ message: "failed" }));
  expect(then).not.toHaveBeenCalled();
  act(() => errorAlerts.acknowledge());
  expect(then).toHaveBeenCalledTimes(1);
  unmount();
});

it("stops listening when the dialog unmounts", () => {
  const { after, unmount } = mountHook();
  const then = vi.fn();
  act(() => errorAlerts.report({ message: "open" }));
  after(then);
  unmount();
  act(() => errorAlerts.acknowledge());
  expect(then).not.toHaveBeenCalled();
});
