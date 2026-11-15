// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { PlatformProvider } from "../../platform/react";
import { DaemonProvider } from "../../store";
import { BROWSER_HOST } from "../../test/hosts";
import { testDaemon } from "../../test/testDaemon";
import { errorAlerts } from "../alerts/errorAlerts";
import { useCopyWithNotice } from "./useCopyWithNotice";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const originalExecCommand = document.execCommand;
beforeEach(() => {
  // The selection copy that follows a refused clipboard write fails too.
  Object.assign(document, { execCommand: vi.fn(() => false) });
});
afterEach(() => {
  Object.assign(document, { execCommand: originalExecCommand });
});

function mountCopy(writeText: (text: string) => Promise<void>) {
  const showNotice = vi.fn();
  let copy!: ReturnType<typeof useCopyWithNotice>;
  function Subject() {
    copy = useCopyWithNotice();
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(
    <PlatformProvider value={{ kind: "browser", host: BROWSER_HOST, clipboard: { writeText } }}>
      <DaemonProvider value={testDaemon({ showNotice })}><Subject /></DaemonProvider>
    </PlatformProvider>,
  ));
  return { copy: (text: string, notice: Parameters<typeof copy>[1]) => act(async () => copy(text, notice)), showNotice };
}

it("titles the error alert with what the caller says failed and states the clipboard as the cause", async () => {
  const { copy, showNotice } = mountCopy(async () => { throw new Error("denied"); });
  await copy("text", { copied: "Path copied", failed: "Could not copy the path" });
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  expect(errorAlerts.getSnapshot()[0]).toMatchObject({
    title: "Could not copy the path",
    message: "The clipboard could not be written to.",
    classification: { kind: "actionable", recovery: "clipboardRetry" },
  });
  expect(showNotice).not.toHaveBeenCalled();
});

it("queues the alert of each of two copies that fail one after the other, each with its own title", async () => {
  const { copy } = mountCopy(async () => { throw new Error("denied"); });
  await copy("a", { failed: "Could not copy the project name" });
  await copy("b", { failed: "Could not copy the session ID" });
  expect(errorAlerts.getSnapshot().map(({ title, message }) => ({ title, message }))).toEqual([
    { title: "Could not copy the project name", message: "The clipboard could not be written to." },
    { title: "Could not copy the session ID", message: "The clipboard could not be written to." },
  ]);
});

it("raises no alert when the text is copied, and says so with the caller's notice", async () => {
  const writeText = vi.fn(async (_text: string) => {});
  const onCopied = vi.fn();
  const { copy, showNotice } = mountCopy(writeText);
  await copy("text", { copied: "Path copied", failed: "Could not copy the path", onCopied });
  expect(writeText).toHaveBeenCalledExactlyOnceWith("text");
  expect(onCopied).toHaveBeenCalledOnce();
  expect(showNotice).toHaveBeenCalledExactlyOnceWith("Path copied");
  expect(errorAlerts.getSnapshot()).toEqual([]);
});
