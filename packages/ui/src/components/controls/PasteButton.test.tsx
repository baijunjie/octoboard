// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { errorAlerts } from "../alerts/errorAlerts";
import type { PlatformAdapter } from "../../platform";
import { ViewerHost } from "../../viewer/viewerTestSupport";
import { DONE_HOLD_MS, RESET_AFTER_TOOLTIP_CLOSE_MS } from "./FeedbackButton";
import { PasteButton } from "./PasteButton";
import { enter, leave, mountForTooltip, openTooltip } from "../hint/tooltipTestSupport";
import { fakeTimers, runOutTimers, runPendingTimers } from "../../test/clock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(props: Partial<React.ComponentProps<typeof PasteButton>> = {}, clipboard?: PlatformAdapter["clipboard"], onPaste = vi.fn()) {
  const container = mountForTooltip(
    <ViewerHost clipboard={clipboard}>
      <PasteButton onPaste={onPaste} failedText="Could not paste the text" {...props} />
    </ViewerHost>,
  );
  return { button: container.querySelector("button"), container, onPaste };
}

function press(element: HTMLElement) {
  act(() => {
    for (const type of ["pointerdown", "mousedown", "pointerup", "mouseup", "click"]) {
      element.dispatchEvent(new MouseEvent(type, { bubbles: true, button: 0 }));
    }
  });
}

const iconClass = (button: Element) => button.querySelector("svg")!.getAttribute("class");
const clipboardOf = (readText?: () => Promise<string>): PlatformAdapter["clipboard"] => ({ writeText: async () => {}, readText });

it("is not drawn where the platform has no clipboard, or one that cannot be read", () => {
  expect(mount().container.querySelector("button")).toBeNull();
  expect(mount({}, clipboardOf()).container.querySelector("button")).toBeNull();
});

it("is an icon button named Paste, with its tooltip on hover", () => {
  fakeTimers();
  const { button } = mount({}, clipboardOf(async () => "x"));
  expect(button!.getAttribute("aria-label")).toBe("Paste");
  expect(button!.textContent).toBe("");
  enter(button!);
  runOutTimers();
  expect(openTooltip()?.textContent).toBe("Paste");
});

it("reads the clipboard only when pressed, hands its text over, and turns the tooltip and name to Pasted with the check icon", async () => {
  fakeTimers();
  const readText = vi.fn(async () => "octoboard+ssh://me@box:22");
  const { button, onPaste } = mount({}, clipboardOf(readText));
  expect(readText).not.toHaveBeenCalled();
  const before = iconClass(button!);
  enter(button!);
  runOutTimers();
  press(button!);
  await act(async () => {});
  expect(readText).toHaveBeenCalledOnce();
  expect(onPaste).toHaveBeenCalledExactlyOnceWith("octoboard+ssh://me@box:22");
  expect(openTooltip()?.textContent).toBe("Pasted");
  expect(button!.getAttribute("aria-label")).toBe("Pasted");
  expect(iconClass(button!)).not.toBe(before);
  expect(document.querySelector('[role="status"]')!.textContent).toBe("Pasted");
});

it("goes back to Paste only after the tooltip has closed, as Copy does", async () => {
  fakeTimers();
  const { button } = mount({}, clipboardOf(async () => "x"));
  enter(button!);
  runOutTimers();
  press(button!);
  await act(async () => {});
  leave(button!);
  runPendingTimers();
  expect(button!.getAttribute("aria-label")).toBe("Pasted");
  act(() => void vi.advanceTimersByTime(RESET_AFTER_TOOLTIP_CLOSE_MS - 1));
  expect(button!.getAttribute("aria-label")).toBe("Pasted");
  act(() => void vi.advanceTimersByTime(1));
  expect(button!.getAttribute("aria-label")).toBe("Paste");
  expect(document.querySelector('[role="status"]')!.textContent).toBe("");
});

it("goes back to Paste after the hold where its tooltip was never open", async () => {
  fakeTimers();
  const { button } = mount({}, clipboardOf(async () => "x"));
  press(button!);
  await act(async () => {});
  expect(button!.getAttribute("aria-label")).toBe("Pasted");
  act(() => void vi.advanceTimersByTime(DONE_HOLD_MS));
  expect(button!.getAttribute("aria-label")).toBe("Paste");
});

it("does nothing, and says nothing, for a clipboard with no text", async () => {
  fakeTimers();
  const { button, onPaste } = mount({}, clipboardOf(async () => ""));
  press(button!);
  await act(async () => {});
  expect(onPaste).not.toHaveBeenCalled();
  expect(button!.getAttribute("aria-label")).toBe("Paste");
  expect(document.querySelector('[role="status"]')!.textContent).toBe("");
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("opens the error alert, titled as the caller says and with no recovery, when the clipboard cannot be read", async () => {
  fakeTimers();
  const { button, onPaste } = mount({}, clipboardOf(async () => {
    throw new Error("denied");
  }));
  press(button!);
  await act(async () => {});
  expect(onPaste).not.toHaveBeenCalled();
  expect(errorAlerts.getSnapshot()[0]).toMatchObject({ title: "Could not paste the text", message: "The clipboard could not be read." });
  expect(button!.getAttribute("aria-label")).toBe("Paste");
});

it("takes the caller's texts and draws its children before the icon", async () => {
  fakeTimers();
  const { button } = mount({ beforePasteText: "Paste details", afterPasteText: "Details pasted", children: "Details", size: "sm" }, clipboardOf(async () => "x"));
  expect(button!.textContent).toBe("Details");
  expect(button!.getAttribute("aria-label")).toBeNull();
  enter(button!);
  runOutTimers();
  expect(openTooltip()?.textContent).toBe("Paste details");
  press(button!);
  await act(async () => {});
  expect(openTooltip()?.textContent).toBe("Details pasted");
});

it("fades the paste glyph back in once it is done, and not on mount", async () => {
  fakeTimers();
  const { button } = mount({}, clipboardOf(async () => "x"));
  expect(iconClass(button!)).not.toContain("animate-copy-icon-in");
  press(button!);
  await act(async () => {});
  act(() => void vi.advanceTimersByTime(DONE_HOLD_MS));
  expect(button!.getAttribute("aria-label")).toBe("Paste");
  expect(iconClass(button!)).toContain("animate-copy-icon-in");
});
