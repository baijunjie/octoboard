// @vitest-environment jsdom
import { act } from "react";
import { afterEach, expect, it, vi } from "vitest";

import { Dialog } from "../dialog/Dialog";
import type { PlatformAdapter } from "../../platform";
import { ViewerHost } from "../../viewer/viewerTestSupport";
import { COPIED_HOLD_MS, CopyButton, RESET_AFTER_TOOLTIP_CLOSE_MS } from "./CopyButton";
import { enter, leave, mountForTooltip, openTooltip } from "../hint/tooltipTestSupport";
import { fakeTimers, runPendingTimers, runOutTimers } from "../../test/clock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(props: Partial<React.ComponentProps<typeof CopyButton>> = {}, clipboard?: PlatformAdapter["clipboard"], error = vi.fn()) {
  const container = mountForTooltip(
    <ViewerHost clipboard={clipboard} notifications={{ notice: vi.fn(), error }}>
      <CopyButton text="the text" failedText="Could not copy the text" {...props} />
    </ViewerHost>,
  );
  return { button: container.querySelector("button"), container, error };
}

function press(element: HTMLElement) {
  act(() => {
    for (const type of ["pointerdown", "mousedown", "pointerup", "mouseup", "click"]) {
      element.dispatchEvent(new MouseEvent(type, { bubbles: true, button: 0 }));
    }
  });
}

const iconClass = (button: Element) => button.querySelector("svg")!.getAttribute("class");

const originalExecCommand = document.execCommand;
afterEach(() => {
  document.execCommand = originalExecCommand;
});

it("is drawn where the platform has no clipboard, and copies through the selection", async () => {
  const execCommand = vi.fn(() => true);
  Object.assign(document, { execCommand });
  const { button, error } = mount();
  expect(button!.getAttribute("aria-label")).toBe("Copy");
  press(button!);
  await act(async () => {});
  expect(execCommand).toHaveBeenCalledWith("copy");
  expect(button!.getAttribute("aria-label")).toBe("Copied");
  expect(error).not.toHaveBeenCalled();
});

it("says so in a notification when there is no clipboard and the selection copy fails", async () => {
  Object.assign(document, { execCommand: vi.fn(() => false) });
  const { button, error } = mount();
  press(button!);
  await act(async () => {});
  expect(error).toHaveBeenCalledWith("Could not copy the text");
  expect(button!.getAttribute("aria-label")).toBe("Copy");
});

it("is an icon button named and described by Copy, with its tooltip on hover", () => {
  fakeTimers();
  const { button } = mount({}, { writeText: async () => {} });
  expect(button!.getAttribute("aria-label")).toBe("Copy");
  expect(button!.textContent).toBe("");
  enter(button!);
  runOutTimers();
  expect(openTooltip()?.textContent).toBe("Copy");
});

it("copies, turns the tooltip and name to Copied and keeps the tooltip open through the press", async () => {
  fakeTimers();
  const writeText = vi.fn(async () => {});
  const { button } = mount({}, { writeText });
  const before = iconClass(button!);
  enter(button!);
  runOutTimers();
  press(button!);
  await act(async () => {});
  expect(writeText).toHaveBeenCalledExactlyOnceWith("the text");
  expect(openTooltip()?.textContent).toBe("Copied");
  expect(button!.getAttribute("aria-label")).toBe("Copied");
  expect(iconClass(button!)).not.toBe(before);
  expect(iconClass(button!)).toContain("animate-copy-icon-in");
  expect(document.querySelector('[role="status"]')!.textContent).toBe("Copied");
});

it("goes back to Copy only after the tooltip has closed, the copy icon fading in again", async () => {
  fakeTimers();
  const { button } = mount({}, { writeText: async () => {} });
  expect(iconClass(button!)).not.toContain("animate-copy-icon-in");
  enter(button!);
  runOutTimers();
  press(button!);
  await act(async () => {});
  leave(button!);
  // The tooltip closes on its own timer, and that close is what arms the reset.
  runPendingTimers();
  expect(button!.getAttribute("aria-label")).toBe("Copied");
  // The reset is armed well inside the hold, so only the close can take it back to Copy here.
  act(() => void vi.advanceTimersByTime(RESET_AFTER_TOOLTIP_CLOSE_MS - 1));
  expect(button!.getAttribute("aria-label")).toBe("Copied");
  act(() => void vi.advanceTimersByTime(1));
  expect(button!.getAttribute("aria-label")).toBe("Copy");
  expect(iconClass(button!)).toContain("animate-copy-icon-in");
  expect(document.querySelector('[role="status"]')!.textContent).toBe("");
});

it("takes the caller's texts and draws its children before the icon", async () => {
  fakeTimers();
  const { button } = mount({ beforeCopyText: "Copy path", afterCopyText: "Path copied", children: "Path", size: "sm" }, { writeText: async () => {} });
  expect(button!.textContent).toBe("Path");
  expect(button!.getAttribute("aria-label")).toBeNull();
  enter(button!);
  runOutTimers();
  expect(openTooltip()?.textContent).toBe("Copy path");
  press(button!);
  await act(async () => {});
  expect(openTooltip()?.textContent).toBe("Path copied");
});

it("falls back to selecting hidden text when the clipboard refuses", async () => {
  const execCommand = vi.fn(() => true);
  Object.assign(document, { execCommand });
  const { button, error } = mount({}, { writeText: async () => Promise.reject(new Error("denied")) });
  press(button!);
  await act(async () => {});
  expect(execCommand).toHaveBeenCalledWith("copy");
  expect(document.body.querySelector("span[style*='opacity']")).toBeNull();
  expect(button!.getAttribute("aria-label")).toBe("Copied");
  expect(error).not.toHaveBeenCalled();
});

it("keeps the tooltip open through a fallback copy, which must not move focus off the button", async () => {
  fakeTimers();
  const focusedWhenCopying: (Element | null)[] = [];
  Object.assign(document, { execCommand: vi.fn(() => focusedWhenCopying.push(document.activeElement) > 0) });
  const { button } = mount({}, { writeText: async () => Promise.reject(new Error("denied")) });
  enter(button!);
  runOutTimers();
  button!.focus();
  press(button!);
  await act(async () => {});
  expect(focusedWhenCopying).toEqual([button]);
  expect(document.activeElement).toBe(button);
  expect(openTooltip()?.textContent).toBe("Copied");
});

it("puts the hidden text inside the modal dialog the button is in, and restores the page's selection", async () => {
  const seen: { parent: string | null; selected: string }[] = [];
  Object.assign(document, {
    execCommand: vi.fn(() => {
      const selection = document.getSelection()!;
      seen.push({ parent: selection.anchorNode!.parentElement!.getAttribute("role"), selected: selection.toString() });
      return true;
    }),
  });
  mountForTooltip(
    <ViewerHost clipboard={{ writeText: async () => Promise.reject(new Error("denied")) }}>
      <Dialog title="File" size="viewer" onClose={() => {}} footer={<span />}>
        <p id="page-text">page text</p>
        <CopyButton text="the text" failedText="Could not copy the text" />
      </Dialog>
    </ViewerHost>,
  );
  const button = document.querySelector<HTMLButtonElement>('[role="dialog"] button[aria-label="Copy"]')!;
  const pageText = document.getElementById("page-text")!;
  const range = document.createRange();
  range.selectNodeContents(pageText);
  button.focus();
  document.getSelection()!.removeAllRanges();
  document.getSelection()!.addRange(range);
  press(button);
  await act(async () => {});
  expect(seen).toEqual([{ parent: "dialog", selected: "the text" }]);
  expect(document.getSelection()!.toString()).toBe("page text");
  expect(document.activeElement).toBe(button);
  expect(button.getAttribute("aria-label")).toBe("Copied");
});

it("says so in a notification when neither way copies, and stays in its first state", async () => {
  Object.assign(document, { execCommand: vi.fn(() => false) });
  const { button, error } = mount({ failedText: "Could not copy the path" }, { writeText: async () => Promise.reject(new Error("denied")) });
  press(button!);
  await act(async () => {});
  expect(error).toHaveBeenCalledWith("Could not copy the path");
  expect(button!.getAttribute("aria-label")).toBe("Copy");
});

it("goes back to Copy once COPIED_HOLD_MS has passed when no tooltip was open at the press", async () => {
  fakeTimers();
  const { button } = mount({}, { writeText: async () => {} });
  press(button!);
  await act(async () => {});
  expect(button!.getAttribute("aria-label")).toBe("Copied");
  act(() => void vi.advanceTimersByTime(COPIED_HOLD_MS - 1));
  expect(button!.getAttribute("aria-label")).toBe("Copied");
  act(() => void vi.advanceTimersByTime(1));
  expect(button!.getAttribute("aria-label")).toBe("Copy");
});
