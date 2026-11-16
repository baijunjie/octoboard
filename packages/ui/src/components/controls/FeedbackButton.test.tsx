// @vitest-environment jsdom
import { Scissors } from "lucide-react";
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";

import { DONE_HOLD_MS, FeedbackButton, RESET_AFTER_TOOLTIP_CLOSE_MS, useDoneFeedback } from "./FeedbackButton";
import { enter, leave, mountForTooltip, openTooltip } from "../hint/tooltipTestSupport";
import { fakeTimers, runOutTimers, runPendingTimers } from "../../test/clock";
import { CopyButton } from "./CopyButton";
import { PasteButton } from "./PasteButton";
import { ViewerHost } from "../../viewer/viewerTestSupport";
import { TestHost } from "../../test/TestHost";
import { testDaemon } from "../../test/testDaemon";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function Probe({ onPress }: { onPress: (done: () => void) => void }) {
  const feedback = useDoneFeedback();
  return <FeedbackButton feedback={feedback} idleText="Cut" doneText="Cut!" icon={Scissors} onPress={() => onPress(feedback.markDone)} />;
}

function mount() {
  const container = mountForTooltip(<TestHost daemon={testDaemon()}><Probe onPress={(done) => done()} /></TestHost>);
  return container.querySelector("button")!;
}

function press(element: HTMLElement) {
  act(() => {
    for (const type of ["pointerdown", "mousedown", "pointerup", "mouseup", "click"]) {
      element.dispatchEvent(new MouseEvent(type, { bubbles: true, button: 0 }));
    }
  });
}

const iconClass = (button: Element) => button.querySelector("svg")!.getAttribute("class");

it("turns its name, tooltip, icon and status text to the done wording on a press, and back after the hold where no tooltip was open", async () => {
  fakeTimers();
  const button = mount();
  // The status region writes nothing until its first-write delay has passed.
  runOutTimers();
  expect(button.getAttribute("aria-label")).toBe("Cut");
  const idle = iconClass(button);
  press(button);
  expect(button.getAttribute("aria-label")).toBe("Cut!");
  expect(iconClass(button)).not.toBe(idle);
  expect(document.querySelector('[role="status"]')!.textContent).toBe("Cut!");
  act(() => void vi.advanceTimersByTime(DONE_HOLD_MS - 1));
  expect(button.getAttribute("aria-label")).toBe("Cut!");
  act(() => void vi.advanceTimersByTime(1));
  expect(button.getAttribute("aria-label")).toBe("Cut");
  expect(document.querySelector('[role="status"]')!.textContent).toBe("");
});

it("fades the idle glyph back in after it was done, and not on mount", () => {
  fakeTimers();
  const button = mount();
  expect(iconClass(button)).not.toContain("animate-copy-icon-in");
  press(button);
  act(() => void vi.advanceTimersByTime(DONE_HOLD_MS));
  expect(button.getAttribute("aria-label")).toBe("Cut");
  expect(iconClass(button)).toContain("animate-copy-icon-in");
});

it("keeps the tooltip open through the press and goes back only once it has closed", () => {
  fakeTimers();
  const button = mount();
  enter(button);
  runOutTimers();
  press(button);
  expect(openTooltip()?.textContent).toBe("Cut!");
  // The hold does not end it while the tooltip is open.
  act(() => void vi.advanceTimersByTime(DONE_HOLD_MS * 2));
  expect(button.getAttribute("aria-label")).toBe("Cut!");
  leave(button);
  runPendingTimers();
  act(() => void vi.advanceTimersByTime(RESET_AFTER_TOOLTIP_CLOSE_MS));
  expect(button.getAttribute("aria-label")).toBe("Cut");
});

it("starts no timer for a done state marked after it was unmounted", () => {
  fakeTimers();
  let markDone!: () => void;
  function Capture({ onMarkDone }: { onMarkDone: (mark: () => void) => void }) {
    const mark = useDoneFeedback().markDone;
    React.useEffect(() => onMarkDone(mark), [mark, onMarkDone]);
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<Capture onMarkDone={(mark) => (markDone = mark)} />));
  act(() => root.unmount());
  container.remove();
  const timers = vi.getTimerCount();
  markDone();
  expect(vi.getTimerCount()).toBe(timers);
});

// StrictMode runs every effect's setup, cleanup and setup again on mount, which must leave the button alive.
it("shows Copied after a successful copy under StrictMode", async () => {
  fakeTimers();
  const writeText = vi.fn(async () => {});
  const container = mountForTooltip(
    <React.StrictMode>
      <ViewerHost clipboard={{ writeText }}>
        <CopyButton text="the text" failedText="Could not copy" />
      </ViewerHost>
    </React.StrictMode>,
  );
  const button = container.querySelector("button")!;
  press(button);
  await act(async () => {});
  expect(writeText).toHaveBeenCalledOnce();
  expect(button.getAttribute("aria-label")).toBe("Copied");
});

it("shows Pasted after a successful paste under StrictMode, and goes back after the hold", async () => {
  fakeTimers();
  const onPaste = vi.fn();
  const container = mountForTooltip(
    <React.StrictMode>
      <ViewerHost clipboard={{ writeText: async () => {}, readText: async () => "pasted" }}>
        <PasteButton onPaste={onPaste} failedText="Could not paste" />
      </ViewerHost>
    </React.StrictMode>,
  );
  const button = container.querySelector("button")!;
  press(button);
  await act(async () => {});
  expect(onPaste).toHaveBeenCalledExactlyOnceWith("pasted");
  expect(button.getAttribute("aria-label")).toBe("Pasted");
  act(() => void vi.advanceTimersByTime(DONE_HOLD_MS));
  expect(button.getAttribute("aria-label")).toBe("Paste");
});
