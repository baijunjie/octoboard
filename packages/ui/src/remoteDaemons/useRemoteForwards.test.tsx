// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import type { RemoteForward, RemoteForwardsCapability } from "../platform";
import { useRemoteForwards } from "./useRemoteForwards";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const forward = (port: number): RemoteForward => ({ remotePort: port, localPort: port, address: `http://localhost:${port}/`, localAddress: `http://localhost:${port}/` });

/** Renders the hook for `capability`; `seen()` is what it returned last. */
function mount(capability: RemoteForwardsCapability | undefined) {
  let seen: RemoteForward[] = [];
  const Subject = ({ of }: { of: RemoteForwardsCapability | undefined }) => {
    seen = useRemoteForwards(of);
    return null;
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(createElement(Subject, { of: capability })));
  return { seen: () => seen, rerender: (of: RemoteForwardsCapability | undefined) => act(() => root.render(createElement(Subject, { of }))) };
}

/** A capability whose listings are answered by hand, in whatever order the test chooses, and which says what it was
 * asked in what order. */
function capability() {
  const calls: string[] = [];
  const answers: ((list: RemoteForward[]) => void)[] = [];
  let listener = () => {};
  const unsubscribe = vi.fn();
  const forwards: RemoteForwardsCapability = {
    list: () => {
      calls.push("list");
      return new Promise((resolve) => answers.push(resolve));
    },
    stop: async () => {},
    onChange: (next) => {
      calls.push("subscribe");
      listener = next;
      return unsubscribe;
    },
  };
  return { forwards, calls, answers, change: () => act(async () => listener()), unsubscribe };
}

it("is empty where there is no capability, as in any window but a remote one", () => {
  expect(mount(undefined).seen()).toEqual([]);
});

it("subscribes to the shell's changes before it lists, so a change between the two is not missed", async () => {
  const shell = capability();
  mount(shell.forwards);
  expect(shell.calls).toEqual(["subscribe", "list"]);
});

it("holds what the shell lists, in its order, and lists again at every change", async () => {
  const shell = capability();
  const hook = mount(shell.forwards);
  await act(async () => shell.answers[0]([forward(3000), forward(4000)]));
  expect(hook.seen().map((f) => f.remotePort)).toEqual([3000, 4000]);
  await shell.change();
  expect(shell.calls).toEqual(["subscribe", "list", "list"]);
  await act(async () => shell.answers[1]([forward(4000)]));
  expect(hook.seen().map((f) => f.remotePort)).toEqual([4000]);
});

it("never lets an answer to an earlier listing replace the answer to a later one", async () => {
  const shell = capability();
  const hook = mount(shell.forwards);
  await shell.change();
  // The second listing answers first; the first, stale, answers after it.
  await act(async () => shell.answers[1]([forward(4000)]));
  await act(async () => shell.answers[0]([forward(3000)]));
  expect(hook.seen().map((f) => f.remotePort)).toEqual([4000]);
});

it("keeps the last list when a listing fails, and takes no answer once it is unmounted or the capability replaced", async () => {
  const failing = capability();
  const hook = mount(failing.forwards);
  await act(async () => failing.answers[0]([forward(3000)]));
  failing.forwards.list = () => Promise.reject(new Error("offline"));
  await failing.change();
  expect(hook.seen().map((f) => f.remotePort)).toEqual([3000]);
  const next = capability();
  hook.rerender(next.forwards);
  expect(failing.unsubscribe).toHaveBeenCalledOnce();
  expect(next.calls).toEqual(["subscribe", "list"]);
  hook.rerender(undefined);
  expect(next.unsubscribe).toHaveBeenCalledOnce();
  expect(hook.seen()).toEqual([]);
  await act(async () => next.answers[0]([forward(5000)]));
  expect(hook.seen()).toEqual([]);
});
