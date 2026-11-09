// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { CodedError } from "../errors/codedError";
import { DEFAULT_SETTINGS } from "../store";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { CloneDirSetting } from "./CloneDirSetting";
import { BROWSER_HOST } from "../test/hosts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn((_body?: { type: string }) => Promise.resolve({}) as Promise<never>);

/** Mounts the field and returns its input, with the keys that bubbled past React's own listener on
 * the container to `<body>`, where a dialog portalled there would be waiting for Escape. */
function mount() {
  const bubbled: string[] = [];
  const record = (event: KeyboardEvent) => bubbled.push(event.key);
  document.body.addEventListener("keydown", record);
  onTestFinished(() => document.body.removeEventListener("keydown", record));
  const daemon = testDaemon({ state: { settings: { ...DEFAULT_SETTINGS, default_clone_dir: "/home/dev/Projects" }, homeDir: "/home/dev" }, request });
  const container = mountHost(<CloneDirSetting />, { daemon });
  const input = container.querySelector("input")!;
  const press = (key: string) =>
    act(() => {
      input.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
    });
  return {
    host: container,
    input,
    store: daemon.store,
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

// The picker opens where the field's text says, as every directory field's does (`pickerStart`): the stored directory
// while nothing is typed, what is typed once it names a place, and the home directory otherwise.
it.each([
  { what: "the stored directory, untouched", typed: undefined, path: "~/Projects" },
  { what: "a typed directory", typed: "/srv/repos", path: "/srv/repos" },
  { what: "the home directory for text that names no place", typed: "repos", path: "~" },
])("opens the directory browser at $what", async ({ typed, path }) => {
  request.mockClear();
  const { input } = mount();
  if (typed !== undefined) {
    act(() => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, typed);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
  }
  await act(async () => browse().click());
  expect(request).toHaveBeenCalledWith({ type: "list_dir", path });
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

it.each([undefined, "/srv/chosen"])("holds a typed draft during native browsing and saves only a chosen directory (%s)", async (picked) => {
  let finish!: (path: string | undefined) => void;
  const pending = new Promise<string | undefined>((resolve) => { finish = resolve; });
  const request = vi.fn(async () => ({ type: "ack" as const, id: null }));
  const pick = vi.fn(() => { input.blur(); return pending; });
  const host = mountHost(<CloneDirSetting />, {
    daemon: testDaemon({ isLocal: true, state: { settings: { ...DEFAULT_SETTINGS, default_clone_dir: "/home/dev/Projects" }, homeDir: "/home/dev" }, request }),
    platform: { kind: "tauri", host: BROWSER_HOST, directoryPicker: { pick } },
  });
  const input = host.querySelector("input")!;
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, " /srv/draft ");
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.focus();
  });
  await act(async () => host.querySelector("button")!.click());
  expect(pick).toHaveBeenCalledExactlyOnceWith({ title: "Choose a directory", initialPath: "/srv/draft" });
  expect(request).not.toHaveBeenCalled();
  await act(async () => finish(picked));
  expect(document.activeElement).toBe(input);
  if (picked) expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", default_clone_dir: picked });
  else {
    expect(request).not.toHaveBeenCalled();
    act(() => input.blur());
    expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", default_clone_dir: "/srv/draft" });
    await act(async () => {});
  }
});

it("bases what is typed on the version of the setting in the store, and shows the store's value after a conflict", async () => {
  request.mockClear();
  const { input, press, store } = mount();
  act(() => store.setState({ settings: { ...store.getState().settings, versions: { default_clone_dir: 3 } } }));
  request.mockRejectedValueOnce(new CodedError("changed elsewhere", "write_conflict", { fields: "default_clone_dir", versions: '{"default_clone_dir":5}' }));
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "/srv/mine");
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  press("Enter");
  await act(async () => {});
  expect(request).toHaveBeenCalledWith({ type: "update_settings", default_clone_dir: "/srv/mine", base_versions: { default_clone_dir: 3 } });
  // The other client's change reaches the store, and the field shows it.
  act(() => store.setState({ settings: { ...store.getState().settings, default_clone_dir: "/srv/theirs", versions: { default_clone_dir: 5 } } }));
  expect(input.value).toBe("/srv/theirs");
});

it("sends a value once while it is unanswered, though Enter and the blur after it both say it", async () => {
  request.mockClear();
  const { input, press } = mount();
  act(() => input.focus());
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "/srv/repos");
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  press("Enter");
  act(() => input.blur());
  expect(request).toHaveBeenCalledTimes(1);
  await act(async () => {});
});

it("keeps the draft when the store's version moves but the value stays the same", () => {
  request.mockClear();
  const { input, store } = mount();
  act(() => store.setState({ settings: { ...store.getState().settings, versions: { default_clone_dir: 3 } } }));
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "/srv/draft");
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  act(() => store.setState({ settings: { ...store.getState().settings, versions: { default_clone_dir: 4 } } }));
  expect(input.value).toBe("/srv/draft");
});

const typeInto = (input: HTMLInputElement, value: string) =>
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
const updates = () => request.mock.calls.filter(([body]) => body?.type === "update_settings");
const settingsOf = (store: ReturnType<typeof mount>["store"], change: Record<string, unknown>) =>
  act(() => store.setState({ settings: { ...store.getState().settings, ...change } as never }));
const status = () => document.body.querySelector("[role=status]")?.textContent;

it("replaces what was typed when the value changes elsewhere, and names the field", async () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  const { input, store } = mount();
  typeInto(input, "/srv/draft");
  settingsOf(store, { default_clone_dir: "/srv/theirs", versions: { default_clone_dir: 4 } });
  expect(input.value).toBe("/srv/theirs");
  await act(async () => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS + 150));
  expect(status()).toBe("Changed elsewhere: Default clone directory. The current value is shown.");
});

it("shows a change elsewhere to a field the user did not touch, without announcing it", async () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  const { input, store } = mount();
  settingsOf(store, { default_clone_dir: "/srv/theirs", versions: { default_clone_dir: 4 } });
  expect(input.value).toBe("/srv/theirs");
  await act(async () => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS + 150));
  expect(status() ?? "").toBe("");
});

/** A first send left unanswered, then a different value saved: the one that waits. */
function sendTwo() {
  request.mockClear();
  let answer!: () => void;
  request.mockImplementationOnce(() => new Promise((resolve) => { answer = () => resolve({} as never); }) as Promise<never>);
  const form = mount();
  settingsOf(form.store, { versions: { default_clone_dir: 3 } });
  typeInto(form.input, "/srv/one");
  form.press("Enter");
  typeInto(form.input, "/srv/two");
  form.press("Enter");
  expect(updates()).toHaveLength(1);
  return { ...form, answer: () => act(async () => answer()) };
}

it("sends a value saved while the first is out at the reply, on the version the store holds then, when the broadcast came first", async () => {
  const { input, store, answer } = sendTwo();
  settingsOf(store, { default_clone_dir: "/srv/one", versions: { default_clone_dir: 4 } });
  expect(input.value).toBe("/srv/two");
  // The second is still out when the first is answered: the field does not fall back to the first value meanwhile.
  request.mockImplementationOnce(() => new Promise<never>(() => {}));
  await answer();
  expect(updates()).toHaveLength(2);
  expect(updates()[1]).toEqual([{ type: "update_settings", default_clone_dir: "/srv/two", base_versions: { default_clone_dir: 4 } }]);
  expect(input.value).toBe("/srv/two");
});

it("sends the waiting value again once the store holds the versions a refusal of its own earlier send names", async () => {
  const { input, store, answer } = sendTwo();
  request.mockRejectedValueOnce(new CodedError("changed elsewhere", "write_conflict", { fields: "default_clone_dir", versions: '{"default_clone_dir":4}' }));
  await answer();
  // The reply came before the broadcast: the second went on the old version and was refused.
  expect(updates()).toHaveLength(2);
  expect(input.value).toBe("/srv/two");
  settingsOf(store, { default_clone_dir: "/srv/one", versions: { default_clone_dir: 4 } });
  await act(async () => {});
  expect(updates()).toHaveLength(3);
  expect(updates()[2]).toEqual([{ type: "update_settings", default_clone_dir: "/srv/two", base_versions: { default_clone_dir: 4 } }]);
});

it("drops a waiting value, shows the stored one and reports it when another client changed the setting meanwhile", async () => {
  const alerts = errorAlerts.getSnapshot().length;
  const { input, store, answer } = sendTwo();
  settingsOf(store, { default_clone_dir: "/srv/theirs", versions: { default_clone_dir: 5 } });
  expect(input.value).toBe("/srv/two");
  await answer();
  expect(updates()).toHaveLength(1);
  expect(input.value).toBe("/srv/theirs");
  expect(errorAlerts.getSnapshot().length).toBe(alerts + 1);
});

it("shows a foreign change after a send that changed nothing, and announces it over what was typed since", async () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  request.mockClear();
  const { input, press, store } = mount();
  typeInto(input, "~/Projects/");
  press("Enter");
  await act(async () => {});
  expect(updates()).toHaveLength(1);
  expect(input.value).toBe("~/Projects");
  typeInto(input, "/srv/typed");
  settingsOf(store, { default_clone_dir: "/srv/theirs", versions: { default_clone_dir: 4 } });
  expect(input.value).toBe("/srv/theirs");
  await act(async () => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS + 150));
  expect(status()).toBe("Changed elsewhere: Default clone directory. The current value is shown.");
});

it("does not take the broadcast of an earlier save for another client's when a second value was sent before it arrived", async () => {
  const alerts = errorAlerts.getSnapshot().length;
  request.mockClear();
  const { input, press, store } = mount();
  settingsOf(store, { versions: { default_clone_dir: 3 } });
  typeInto(input, "/srv/one");
  press("Enter");
  await act(async () => {});
  let answer!: () => void;
  request.mockImplementationOnce(() => new Promise((resolve) => { answer = () => resolve({} as never); }) as Promise<never>);
  typeInto(input, "/srv/two");
  press("Enter");
  expect(updates()).toHaveLength(2);
  // The first save's broadcast arrives only now, with the second out.
  settingsOf(store, { default_clone_dir: "/srv/one", versions: { default_clone_dir: 4 } });
  expect(input.value).toBe("/srv/two");
  await act(async () => answer());
  settingsOf(store, { default_clone_dir: "/srv/two", versions: { default_clone_dir: 5 } });
  expect(input.value).toBe("/srv/two");
  expect(errorAlerts.getSnapshot().length).toBe(alerts);
  // Nothing explains the stored value now: a later change to a value this window once sent is another client's.
  typeInto(input, "/srv/typed");
  settingsOf(store, { default_clone_dir: "/srv/one", versions: { default_clone_dir: 6 } });
  expect(input.value).toBe("/srv/one");
});

it("still sends a waiting value after the field has gone, and reports a failure of it", async () => {
  const alerts = errorAlerts.getSnapshot().length;
  const { host, store, answer } = sendTwo();
  settingsOf(store, { default_clone_dir: "/srv/one", versions: { default_clone_dir: 4 } });
  request.mockRejectedValueOnce(new CodedError("no", "other_failure", {}));
  host.render(<></>);
  await answer();
  expect(updates()).toHaveLength(2);
  expect(updates()[1]).toEqual([{ type: "update_settings", default_clone_dir: "/srv/two", base_versions: { default_clone_dir: 4 } }]);
  expect(errorAlerts.getSnapshot().length).toBe(alerts + 1);
});
