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

function mount(initial: Partial<State> = {}) {
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
      <NewSessionButton label="New session" quickLabel="New session with defaults" target={() => request} onOpen={onOpen} onQuickCreate={onQuickCreate} />
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
