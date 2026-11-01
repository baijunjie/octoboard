// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { useSerialPush } from "./useSerialPush";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(): (send: () => Promise<unknown>) => void {
  let push!: (send: () => Promise<unknown>) => void;
  function Host() {
    push = useSerialPush();
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<Host />));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  return push;
}

const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

it("starts a push only when the one before it has settled, so the shell sees them in order", async () => {
  const push = mount();
  const seen: string[] = [];
  let settleFirst!: () => void;
  push(() => {
    seen.push("first sent");
    return new Promise<void>((resolve) => { settleFirst = resolve; }).then(() => { seen.push("first settled"); });
  });
  push(async () => { seen.push("second sent"); });
  await flush();
  expect(seen).toEqual(["first sent"]);
  settleFirst();
  await flush();
  expect(seen).toEqual(["first sent", "first settled", "second sent"]);
});

it("keeps sending after a push that rejects", async () => {
  const push = mount();
  const seen: string[] = [];
  push(() => Promise.reject(new Error("refused")));
  push(async () => { seen.push("second"); });
  await flush();
  expect(seen).toEqual(["second"]);
});

it("keeps sending after a push that throws as it is sent", async () => {
  const push = mount();
  const seen: string[] = [];
  push(() => { throw new Error("broken"); });
  push(async () => { seen.push("second"); });
  await flush();
  expect(seen).toEqual(["second"]);
});
