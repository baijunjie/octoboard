// @vitest-environment jsdom
import React, { act, Suspense } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { retryableLazy } from "./retryableLazy";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

class Catch extends React.Component<{ children: React.ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError(): { failed: boolean } {
    return { failed: true };
  }
  render(): React.ReactNode {
    return this.state.failed ? "failed" : this.props.children;
  }
}

async function mount(node: React.ReactNode): Promise<{ container: HTMLElement; unmount: () => Promise<void> }> {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  let mounted = true;
  const unmount = async () => {
    if (!mounted) return;
    mounted = false;
    await act(async () => root.unmount());
    container.remove();
  };
  onTestFinished(unmount);
  await act(async () => root.render(node));
  return { container, unmount };
}

it("passes its props to the loaded component", async () => {
  const { Component } = retryableLazy(async () => ({ default: ({ name }: { name: string }) => <p>hello {name}</p> }));
  const { container } = await mount(
    <Suspense fallback={null}>
      <Component name="there" />
    </Suspense>,
  );
  expect(container.textContent).toBe("hello there");
});

it("imports again after a retry, where a plain lazy would keep the failure", async () => {
  let loads = 0;
  const { Component, retry } = retryableLazy(async () => {
    loads += 1;
    if (loads === 1) throw new Error("chunk");
    return { default: () => <p>loaded</p> };
  });
  const tree = (
    <Catch>
      <Suspense fallback={null}>
        <Component />
      </Suspense>
    </Catch>
  );
  const first = await mount(tree);
  expect(first.container.textContent).toBe("failed");
  await first.unmount();

  const unretried = await mount(tree);
  expect(unretried.container.textContent).toBe("failed");
  expect(loads).toBe(1);
  await unretried.unmount();

  retry();
  const retried = await mount(tree);
  expect(retried.container.textContent).toBe("loaded");
  expect(loads).toBe(2);
});

it("does nothing on a retry after a successful load, so a mounted instance stays", async () => {
  let loads = 0;
  let mounts = 0;
  const { Component, retry } = retryableLazy(async () => {
    loads += 1;
    return {
      default: function Loaded() {
        React.useEffect(() => {
          mounts += 1;
        }, []);
        return <p>loaded</p>;
      },
    };
  });
  const { container } = await mount(
    <Suspense fallback={null}>
      <Component />
    </Suspense>,
  );
  expect(container.textContent).toBe("loaded");
  retry();
  await act(async () => {});
  expect(loads).toBe(1);
  expect(mounts).toBe(1);
});
