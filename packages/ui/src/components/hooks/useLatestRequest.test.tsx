// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { createBrowseLane } from "../../browse/lane";
import { DaemonRequestError } from "../../daemon-client";
import { DaemonProvider } from "../../store";
import { testDaemon } from "../../test/testDaemon";
import { useLatestRequest, type LatestRequest, type LatestRequestSend } from "./useLatestRequest";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

type Held = { state: "loading" } | { state: "loaded"; answer: string } | { state: "error"; kind: string } | { state: "lost" };

/** A request the test answers itself. */
type Out = { resolve: (answer: string) => void; reject: (err: unknown) => void };

function mount(onReconnect?: (last: { asked: boolean; lost: boolean }) => void) {
  const daemon = testDaemon({ state: { connectionState: "open", snapshotEpoch: 1 }, request: vi.fn() });
  const { store } = daemon;
  let latest!: LatestRequest<Held>;
  let renders = 0;
  let scope = "a";
  const Probe = () => {
    renders++;
    latest = useLatestRequest<Held>({ scope, initial: { state: "loading" }, onReconnect });
    return null;
  };
  const root = createRoot(document.body.appendChild(document.createElement("div")));
  const render = () => act(() => root.render(<DaemonProvider value={daemon}><Probe /></DaemonProvider>));
  render();
  onTestFinished(() => act(() => root.unmount()));
  const out: Out[] = [];
  const send = (extra: Partial<LatestRequestSend<Held, string>> = {}) => {
    const sent = latest.send<string>({
      request: () => new Promise<string>((resolve, reject) => void out.push({ resolve, reject })),
      accept: (reply) => (reply === "other" ? undefined : (held) => (held.state === "loaded" && held.answer === reply ? held : { state: "loaded", answer: reply })),
      failed: (failure) => ({ state: "error", kind: failure.kind }),
      disconnected: () => ({ state: "lost" }),
      ...extra,
    });
    return sent;
  };
  return {
    out,
    send,
    value: () => latest.value,
    latest: () => latest,
    reconnects: () => latest.reconnects,
    renders: () => renders,
    scope: (next: string) => {
      scope = next;
      render();
    },
    reconnect: () => act(() => store.setState((state) => ({ snapshotEpoch: state.snapshotEpoch + 1 }))),
  };
}

const daemonError = (code: string) => new DaemonRequestError(code, code, {});

it("takes the latest request's answer only, whichever answers last", async () => {
  const hook = mount();
  act(() => void hook.send());
  act(() => void hook.send());
  await act(async () => hook.out[1]!.resolve("second"));
  await act(async () => hook.out[0]!.resolve("first"));
  expect(hook.value()).toEqual({ state: "loaded", answer: "second" });
});

it("holds an answer for the scope it was asked for, and shows the initial value for another scope", async () => {
  const hook = mount();
  act(() => void hook.send());
  hook.scope("b");
  await act(async () => hook.out[0]!.resolve("for a"));
  expect(hook.value()).toEqual({ state: "loading" });
  act(() => void hook.send());
  await act(async () => hook.out[1]!.resolve("for b"));
  expect(hook.value()).toEqual({ state: "loaded", answer: "for b" });
  hook.scope("a");
  expect(hook.value()).toEqual({ state: "loading" });
});

it("drops a reply that does not answer the request, and publishes nothing for an unchanged answer", async () => {
  const hook = mount();
  act(() => void hook.send());
  await act(async () => hook.out[0]!.resolve("other"));
  expect(hook.value()).toEqual({ state: "loading" });
  act(() => void hook.send());
  await act(async () => hook.out[1]!.resolve("same"));
  const renders = hook.renders();
  const held = hook.value();
  act(() => void hook.send());
  await act(async () => hook.out[2]!.resolve("same"));
  expect(hook.value()).toBe(held);
  expect(hook.renders()).toBe(renders);
});

it("drops a superseded request, sends a changed one again before showing it, and shows any other failure", async () => {
  const hook = mount();
  act(() => void hook.send());
  await act(async () => hook.out[0]!.reject(daemonError("request_superseded")));
  expect(hook.value()).toEqual({ state: "loading" });

  act(() => void hook.send());
  for (let attempt = 1; attempt <= 3; attempt++) await act(async () => hook.out[attempt]!.reject(daemonError("source_changed")));
  expect(hook.out).toHaveLength(4);
  expect(hook.value()).toEqual({ state: "error", kind: "changed" });

  act(() => void hook.send());
  await act(async () => hook.out[4]!.reject(daemonError("git_failed")));
  expect(hook.value()).toEqual({ state: "error", kind: "failed" });
});

it("keeps what is held through a lost request and tells the reconnect it was lost", async () => {
  const reconnects: { asked: boolean; lost: boolean }[] = [];
  const hook = mount((last) => reconnects.push(last));
  hook.reconnect();
  act(() => void hook.send());
  await act(async () => hook.out[0]!.reject(daemonError("connection_closed")));
  expect(hook.value()).toEqual({ state: "lost" });
  hook.reconnect();
  expect(reconnects).toEqual([{ asked: false, lost: false }, { asked: true, lost: true }]);
});

it("counts the reconnects that found the latest request lost, for the caller to ask again", async () => {
  const hook = mount();
  hook.reconnect();
  expect(hook.reconnects()).toBe(0);
  act(() => void hook.send());
  hook.reconnect();
  // The request is still out, so it is not lost.
  expect(hook.reconnects()).toBe(0);
  await act(async () => hook.out[0]!.reject(daemonError("connection_closed")));
  hook.reconnect();
  expect(hook.reconnects()).toBe(1);
  act(() => void hook.send());
  await act(async () => hook.out[1]!.resolve("answer"));
  hook.reconnect();
  expect(hook.reconnects()).toBe(1);
});

it("takes a scope given as undefined for the scope undefined, and no scope for the current one", async () => {
  const hook = mount();
  act(() => hook.latest().hold(() => ({ state: "loaded", answer: "nowhere" }), undefined));
  expect(hook.value()).toEqual({ state: "loading" });
  act(() => hook.latest().hold(() => ({ state: "loaded", answer: "here" })));
  expect(hook.value()).toEqual({ state: "loaded", answer: "here" });
  act(() => void hook.send({ scope: undefined }));
  // Asked for the scope undefined, not the hook's: the reply is not taken.
  await act(async () => hook.out[0]!.resolve("for nothing"));
  expect(hook.value()).toEqual({ state: "loaded", answer: "here" });
  act(() => void hook.send());
  await act(async () => hook.out[1]!.resolve("current"));
  expect(hook.value()).toEqual({ state: "loaded", answer: "current" });
});

it("words a failure with the current translation function", async () => {
  const hook = mount();
  let given: unknown;
  act(() => void hook.send({ failed: (failure, _held, t) => ((given = t), { state: "error", kind: failure.kind }) }));
  await act(async () => hook.out[0]!.reject(daemonError("git_failed")));
  expect(given).toBeTypeOf("function");
});

it("holds what the request says while it is out, and tells a poll it is busy and when it was sent", async () => {
  const hook = mount();
  expect(hook.latest().asked()).toBe(false);
  act(() => void hook.send({ pending: () => ({ state: "lost" }) }));
  expect(hook.value()).toEqual({ state: "lost" });
  expect(hook.latest().busy()).toBe(true);
  expect(hook.latest().sentAt()).toBeGreaterThan(0);
  await act(async () => hook.out[0]!.resolve("done"));
  expect(hook.latest().busy()).toBe(false);
  expect(hook.latest().asked()).toBe(true);
});

it("waits in a lane, and skips a request whose turn comes after a newer one took its place", async () => {
  const lane = createBrowseLane(1);
  const blocker = new Promise<void>(() => {});
  void lane.send(() => blocker);
  const hook = mount();
  act(() => void hook.send({ lane }));
  act(() => void hook.send({ lane }));
  expect(hook.out).toHaveLength(0);
});

it("leaves a rejection the caller handles to the caller, and drops everything on reset", async () => {
  const hook = mount();
  act(() => void hook.send({ handled: () => true }));
  await act(async () => hook.out[0]!.reject(daemonError("git_failed")));
  expect(hook.value()).toEqual({ state: "loading" });
  act(() => void hook.send());
  await act(async () => hook.out[1]!.resolve("kept"));
  act(() => void hook.send());
  act(() => hook.latest().reset());
  await act(async () => hook.out[2]!.resolve("late"));
  expect(hook.value()).toEqual({ state: "loading" });
});
