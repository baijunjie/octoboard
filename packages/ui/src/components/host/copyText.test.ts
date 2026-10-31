// @vitest-environment jsdom
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { copyText } from "./copyText";

const originalExecCommand = document.execCommand;
afterEach(() => {
  document.execCommand = originalExecCommand;
});

it("writes the text as text/plain and nothing else when the document's copy command runs", async () => {
  const written = new Map<string, string>();
  let prevented = false;
  Object.assign(document, {
    execCommand: vi.fn(() => {
      const event = new Event("copy", { cancelable: true }) as ClipboardEvent;
      Object.defineProperty(event, "clipboardData", { value: { setData: (type: string, data: string) => written.set(type, data) } });
      document.body.dispatchEvent(event);
      prevented = event.defaultPrevented;
      return true;
    }),
  });
  await copyText("line one\nline two", {});
  expect([...written]).toEqual([["text/plain", "line one\nline two"]]);
  expect(prevented).toBe(true);
});

it("takes its copy listener off afterwards, and rejects when the command does not work", async () => {
  Object.assign(document, { execCommand: vi.fn(() => false) });
  const remove = vi.spyOn(document, "removeEventListener");
  onTestFinished(() => remove.mockRestore());
  await expect(copyText("x", {})).rejects.toThrow("The text could not be copied");
  expect(remove).toHaveBeenCalledWith("copy", expect.any(Function), true);
});
