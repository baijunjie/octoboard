// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { AGENT_OPTIONS } from "../agents";
import type { NewSessionRequest } from "../dialogs/dialogRequest";
import { consoleOf } from "../gallery/fixtures/builders";
import { createStateStore, DaemonProvider, type Daemon, type State } from "../store";
import { IS_MAC } from "../useWindowShortcut";
import { NewSessionButton } from "./NewSessionButton";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request: NewSessionRequest = { kind: "new-console-session", console: consoleOf("c-1", "Main") };

/** The platform's shortcut modifier, as a keyboard event's init. */
const MODIFIER = IS_MAC ? { metaKey: true } : { ctrlKey: true };

function mount(initial: Partial<State> = {}, shortcut = false) {
  const daemon: Daemon = {
    store: createStateStore(initial),
    request: vi.fn(),
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  const onOpen = vi.fn();
  const onQuickCreate = vi.fn();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(
    <DaemonProvider value={daemon}>
      <NewSessionButton shortcut={shortcut} label="New session" quickLabel="New session with defaults" target={() => request} onOpen={onOpen} onQuickCreate={onQuickCreate} />
    </DaemonProvider>,
  ));
  onTestFinished(() => {
    // The held state outlives the button, so a test that leaves it down must not carry it on.
    act(() => void window.dispatchEvent(new Event("blur")));
    act(() => root.unmount());
    container.remove();
  });
  const button = () => container.querySelector("button")!;
  const key = (type: "keydown" | "keyup", held: boolean) =>
    act(() => void window.dispatchEvent(new KeyboardEvent(type, held ? MODIFIER : {})));
  const click = (init: MouseEventInit = {}) => act(() => void button().dispatchEvent(new MouseEvent("click", { bubbles: true, ...init })));
  return { button, key, click, onOpen, onQuickCreate };
}

const noAgent = new Map(AGENT_OPTIONS.map(({ value }) => [value, { agent: value, availability: "unavailable" as const }]));

it("does not register creation shortcuts outside a focus header", () => {
  const { onOpen, onQuickCreate } = mount();
  const event = new KeyboardEvent("keydown", { code: "KeyN", key: "n", ...MODIFIER, cancelable: true });
  act(() => void window.dispatchEvent(event));
  expect(event.defaultPrevented).toBe(false);
  expect(onOpen).not.toHaveBeenCalled();
  expect(onQuickCreate).not.toHaveBeenCalled();
});

it("does not run creation shortcuts without an available agent", () => {
  const { onOpen, onQuickCreate } = mount({ agentAvailability: noAgent }, true);
  for (const shiftKey of [false, true]) {
    act(() => void window.dispatchEvent(new KeyboardEvent("keydown", { code: "KeyN", key: "n", ...MODIFIER, shiftKey })));
  }
  expect(onOpen).not.toHaveBeenCalled();
  expect(onQuickCreate).not.toHaveBeenCalled();
});

it("turns into the quick-create button while the modifier is held and back when it is let go", () => {
  const { button, key } = mount();
  expect(button().getAttribute("aria-label")).toBe("New session");
  key("keydown", true);
  expect(button().getAttribute("aria-label")).toBe("New session with defaults");
  key("keyup", false);
  expect(button().getAttribute("aria-label")).toBe("New session");
});

it("opens the dialog on a plain press and quick-creates on a press while held", () => {
  const { click, key, onOpen, onQuickCreate } = mount();
  click();
  expect(onOpen).toHaveBeenCalledWith(request);
  expect(onQuickCreate).not.toHaveBeenCalled();
  key("keydown", true);
  click();
  expect(onQuickCreate).toHaveBeenCalledWith(request);
  expect(onOpen).toHaveBeenCalledTimes(1);
});

it("quick-creates on a press carrying the modifier even when the key state was missed", () => {
  const { click, onOpen, onQuickCreate } = mount();
  click(MODIFIER);
  expect(onQuickCreate).toHaveBeenCalledWith(request);
  expect(onOpen).not.toHaveBeenCalled();
});

it("is a disabled \"+\" that opens nothing, even on a modified press, with no agent added and installed", () => {
  const { button, key, click, onOpen, onQuickCreate } = mount({ agentAvailability: noAgent });
  key("keydown", true);
  expect(button().getAttribute("aria-label")).toBe("New session");
  expect(button().disabled).toBe(true);
  click(MODIFIER);
  expect(onOpen).not.toHaveBeenCalled();
  expect(onQuickCreate).not.toHaveBeenCalled();
});

const available = new Map(AGENT_OPTIONS.map(({ value }) => [value, { agent: value, availability: "available" as const }]));
const spent = AGENT_OPTIONS.map(({ value }) => ({
  agent: value, account: null, fetched_at: 0, plan: null, unavailable: null,
  windows: [{ kind: "weekly" as const, label: null, used_percent: 100, resets_at: null, window_minutes: null }],
}));

it("keeps the quick form focusable, named for the action and described by why it cannot create, when every account is used up", () => {
  const { button, key, click, onOpen, onQuickCreate } = mount({ agentAvailability: available, accountQuotas: spent });
  button().focus();
  key("keydown", true);
  expect(document.activeElement).toBe(button());
  expect(button().disabled).toBe(false);
  expect(button().getAttribute("aria-disabled")).toBe("true");
  expect(button().getAttribute("aria-label")).toBe("New session with defaults");
  const reason = document.getElementById(button().getAttribute("aria-describedby")!);
  expect(reason?.textContent).toBe("No available account");
  // The press goes to the quick create, which refuses with the reason; it does not open a session.
  click();
  expect(onQuickCreate).toHaveBeenCalledWith(request);
  expect(onOpen).not.toHaveBeenCalled();
  key("keyup", false);
  expect(button().getAttribute("aria-disabled")).toBeNull();
  expect(document.activeElement).toBe(button());
});
