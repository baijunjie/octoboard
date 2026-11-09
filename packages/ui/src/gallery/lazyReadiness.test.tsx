// @vitest-environment jsdom
import React, { act, Suspense, useEffect } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";
import { retryableLazy } from "../components/hooks/retryableLazy";
import { observeLazyReadiness, trackSuspenseFallback } from "../components/hooks/lazyReadiness";
import { useDialogAction } from "../components/dialog/dialogHooks";
import { RendererBoundary } from "../viewer/code/codeRenderer";
import { fakeTimers } from "../test/clock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The budgets `settle` is given are armed on the fake clock, and each test resolves its deferreds by hand: what a test
// sees depends on what it resolved, never on how long the machine took between arming a budget and resolving.
beforeEach(() => {
  fakeTimers();
});

function deferred<T>() {
  let resolve!: (value: T) => void, reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

async function setup(node: () => React.ReactNode) {
  const tracking = observeLazyReadiness();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container, { onCaughtError: tracking.caught });
  onTestFinished(async () => {
    await act(async () => root.unmount());
    container.remove();
    tracking.dispose();
  });
  await act(async () => root.render(node()));
  return { tracking, container, render: async (next: React.ReactNode) => { await act(async () => root.render(next)); } };
}

it("keeps the production fallback identical when capture observation is disabled", () => {
  const fallback = <p>Loading</p>;
  expect(trackSuspenseFallback(fallback)).toBe(fallback);
  expect(trackSuspenseFallback(null)).toBeNull();
});

it("waits for delayed lazy content and its committed nested child in otherwise identical captures", async () => {
  const screenshots: string[] = [];
  for (let run = 0; run < 2; run++) {
    const parent = deferred<{ default: () => React.ReactElement }>();
    const child = deferred<{ default: () => React.ReactElement }>();
    const Parent = retryableLazy(() => parent.promise).Component;
    const Child = retryableLazy(() => child.promise).Component;
    const { tracking, container } = await setup(() => <Suspense fallback={trackSuspenseFallback(null)}><Parent /></Suspense>);
    let ready = false;
    const capture = tracking.settle(async () => {}, 1000).then(() => { ready = true; screenshots.push(container.innerHTML); });
    await Promise.resolve();
    expect(ready).toBe(false);
    expect(container.textContent).toBe("");
    await act(async () => parent.resolve({ default: () => <Child /> }));
    expect(ready).toBe(false);
    await act(async () => child.resolve({ default: () => <section><h2>Files</h2><div role="treegrid">README.md</div></section> }));
    await capture;
    expect(ready).toBe(true);
    expect(container.querySelector('[role="treegrid"]')?.textContent).toBe("README.md");
    tracking.dispose();
  }
  expect(screenshots[0]).toBe(screenshots[1]);
});

it("fails explicitly within the budget if a visible lazy module never loads", async () => {
  const budget = 50;
  const Lazy = retryableLazy(() => new Promise<{ default: () => null }>(() => {})).Component;
  const { tracking } = await setup(() => <Suspense fallback={trackSuspenseFallback(null)}><Lazy /></Suspense>);
  const result = tracking.settle(async () => {}, budget).catch((error: Error) => error.message);
  await vi.advanceTimersByTimeAsync(budget);
  expect(await result).toBe(`Lazy content did not settle within ${budget}ms (1 Suspense boundaries pending)`);
});

it("discards a suspended view that closes, including a later rejection of its abandoned import", async () => {
  const load = deferred<{ default: () => null }>();
  const Lazy = retryableLazy(() => load.promise).Component;
  const { tracking, render } = await setup(() => <Suspense fallback={trackSuspenseFallback(null)}><Lazy /></Suspense>);
  const ready = tracking.settle(async () => {}, 1000);
  await render(<p>The overlay closed</p>);
  await ready;
  await act(async () => load.reject(new Error("abandoned import")));
  await expect(tracking.settle(async () => {}, 1000)).resolves.toBeUndefined();
});

class Catch extends React.Component<{ children: React.ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  render() { return this.state.failed ? <p>Load failed</p> : this.props.children; }
}

it("reports a failed visible import and clears that failure when replacement content commits", async () => {
  const logged = vi.spyOn(console, "error").mockImplementation(() => {});
  onTestFinished(() => logged.mockRestore());
  const first = deferred<{ default: () => React.ReactElement }>();
  let attempts = 0;
  const { Component, retry } = retryableLazy(() => ++attempts === 1 ? first.promise : Promise.resolve({ default: () => <p>Recovered</p> }));
  const tree = (key: string) => <Catch key={key}><Suspense fallback={trackSuspenseFallback(null)}><Component /></Suspense></Catch>;
  const { tracking, container, render } = await setup(() => tree("first"));
  await act(async () => first.reject(new Error("module unavailable")));
  await expect(tracking.settle(async () => {}, 1000)).rejects.toThrow("Lazy content failed to load: module unavailable");
  retry();
  await render(tree("retry"));
  await tracking.settle(async () => {}, 1000);
  expect(container.textContent).toBe("Recovered");
});

it("captures a real dialog action's intentional pending request without waiting for it to complete", async () => {
  function BusyDialog() {
    const { busy, run } = useDialogAction(undefined, "Save failed");
    useEffect(() => { void run(() => new Promise<void>(() => {})); }, []);
    return <button disabled={busy} aria-busy={busy}>Save</button>;
  }
  const { tracking, container } = await setup(() => <BusyDialog />);
  await tracking.settle(async () => {}, 1000);
  expect(container.querySelector("button")?.getAttribute("aria-busy")).toBe("true");
  expect(container.querySelector("button")?.disabled).toBe(true);
});

it("does not mistake an abandoned rejection for another visible boundary's failure", async () => {
  const abandoned = deferred<{ default: () => null }>();
  const visible = deferred<{ default: () => React.ReactElement }>();
  const Old = retryableLazy(() => abandoned.promise).Component;
  const Kept = retryableLazy(() => visible.promise).Component;
  const kept = () => <Suspense key="kept" fallback={trackSuspenseFallback(null)}><Kept /></Suspense>;
  const { tracking, container, render } = await setup(() => <><Suspense key="old" fallback={trackSuspenseFallback(null)}><Old /></Suspense>{kept()}</>);
  await render(<>{kept()}</>);
  await act(async () => abandoned.reject(new Error("closed overlay")));
  const ready = tracking.settle(async () => {}, 1000);
  await act(async () => visible.resolve({ default: () => <p>Visible content</p> }));
  await ready;
  expect(container.textContent).toBe("Visible content");
});

it("recognizes an immediately rejected module even if React never committed its fallback", async () => {
  const Lazy = retryableLazy(() => Promise.reject(new Error("immediate module rejection"))).Component;
  const { tracking } = await setup(() => <Catch><Suspense fallback={trackSuspenseFallback(null)}><Lazy /></Suspense></Catch>);
  await expect(tracking.settle(async () => {}, 1000)).rejects.toThrow("Lazy content failed to load: immediate module rejection");
});

it("retains a caught failure when RendererBoundary only resets the lazy cache, until a replacement commits", async () => {
  const warned = vi.spyOn(console, "warn").mockImplementation(() => {});
  onTestFinished(() => warned.mockRestore());
  const first = deferred<{ default: () => React.ReactElement }>();
  const replacement = deferred<{ default: () => React.ReactElement }>();
  let attempts = 0;
  const { Component, retry } = retryableLazy(() => ++attempts === 1 ? first.promise : replacement.promise);
  const tree = (key: string) => <RendererBoundary resetKey={key} fallback={<p>Plain fallback</p>} retry={retry}><Suspense fallback={trackSuspenseFallback(null)}><Component /></Suspense></RendererBoundary>;
  const { tracking, container, render } = await setup(() => tree("failed"));
  await act(async () => first.reject(new Error("renderer module unavailable")));
  expect(container.textContent).toBe("Plain fallback");
  expect(attempts).toBe(1);
  await expect(tracking.settle(async () => {}, 1000)).rejects.toThrow("Lazy content failed to load: renderer module unavailable");
  await render(tree("replacement"));
  expect(attempts).toBe(2);
  await expect(tracking.settle(async () => {}, 1000)).rejects.toThrow("Lazy content failed to load: renderer module unavailable");
  await act(async () => replacement.resolve({ default: () => <p>Committed replacement</p> }));
  await tracking.settle(async () => {}, 1000);
  expect(container.textContent).toBe("Committed replacement");
});
