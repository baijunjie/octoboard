// @vitest-environment jsdom
import { act } from "react";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { LanguageProvider } from "../../i18n/react";
import { errorAlerts } from "./errorAlerts";
import { PlatformProvider } from "../../platform/react";
import { activate, mountForTooltip } from "../hint/tooltipTestSupport";
import { DiagnosticBlock, DiagnosticLabelContext } from "./DiagnosticBlock";
import { BROWSER_HOST } from "../../test/hosts";
import { ViewerHost } from "../../viewer/viewerTestSupport";
import { fakeTimers, runOutTimers } from "../../test/clock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const originalExecCommand = document.execCommand;
afterEach(() => {
  document.execCommand = originalExecCommand;
});

const TEXT = "  first line\r\n\tindented line\n" + "a long message ".repeat(100) + "\n\n";

function mount({ text = TEXT, labelledBy, disclosure, dir, wrap }: { text?: string; labelledBy?: string; disclosure?: string; dir?: "ltr" | "auto"; wrap?: boolean } = {}) {
  // Before anything mounts: the frame HeroUI's overflow watch reports from is armed on the faked clock.
  fakeTimers();
  const writeText = vi.fn(async (_text: string) => {});
  // No daemon: diagnostics must still work when startup fails before a connection exists.
  const container = mountForTooltip(
    <PlatformProvider value={{ kind: "browser", host: BROWSER_HOST, clipboard: { writeText } }}>
      <LanguageProvider>
        <DiagnosticLabelContext.Provider value={disclosure}>
          <DiagnosticBlock labelledBy={labelledBy} dir={dir} wrap={wrap}>{text}</DiagnosticBlock>
        </DiagnosticLabelContext.Provider>
      </LanguageProvider>
    </PlatformProvider>,
  );
  return { container, writeText, scroller: container.querySelector<HTMLElement>("[data-slot=scroll-shadow]")! };
}

it("keeps the original text without automatic wrapping inside a bounded two-way scroller", () => {
  const { container, scroller } = mount();
  const pre = container.querySelector("pre")!;
  expect(pre.textContent).toBe(TEXT);
  expect(pre.classList.contains("whitespace-pre")).toBe(true);
  expect(pre.classList.contains("whitespace-pre-wrap")).toBe(false);
  expect(scroller.classList.contains("max-h-[40vh]")).toBe(true);
  expect(scroller.classList.contains("overflow-auto")).toBe(true);
  const button = container.querySelector<HTMLButtonElement>('button[aria-label="Copy"]')!;
  expect(button).not.toBeNull();
  expect(scroller.contains(button)).toBe(false);
});

it("copies the complete original text including indentation, line endings and trailing blank lines without a daemon", async () => {
  const { container, writeText, scroller } = mount();
  scroller.scrollTop = 100;
  scroller.scrollLeft = 100;
  activate(container.querySelector<HTMLButtonElement>('button[aria-label="Copy"]')!);
  await act(async () => {});
  expect(writeText).toHaveBeenCalledExactlyOnceWith(TEXT);
  expect(container.querySelector('button[aria-label="Copied"]')).not.toBeNull();
});

it("keeps the scrollbar at the physical right while user text can set its own reading direction", () => {
  const diagnostic = mount();
  expect(diagnostic.scroller.getAttribute("dir")).toBe("ltr");
  expect(diagnostic.container.querySelector("pre")!.getAttribute("dir")).toBe("ltr");
  const own = mount({ dir: "auto" });
  expect(own.scroller.getAttribute("dir")).toBe("ltr");
  expect(own.container.querySelector("pre")!.getAttribute("dir")).toBe("auto");
});

it.each([TEXT, "远程的菜单栏图标中\n还没有改完的那一行"])("wraps the long lines of text of the user's own on screen and still copies %j unchanged", async (text) => {
  const { container, writeText } = mount({ text, dir: "auto", wrap: true });
  const pre = container.querySelector("pre")!;
  expect(pre.classList.contains("whitespace-pre-wrap")).toBe(true);
  expect(pre.classList.contains("whitespace-pre")).toBe(false);
  expect(pre.textContent).toBe(text);
  activate(container.querySelector<HTMLButtonElement>('button[aria-label="Copy"]')!);
  await act(async () => {});
  expect(writeText).toHaveBeenCalledExactlyOnceWith(text);
});

function layout(width: number, height: number): void {
  for (const [key, value] of Object.entries({ clientWidth: 100, clientHeight: 100, scrollWidth: width, scrollHeight: height })) {
    const mock = vi.spyOn(HTMLElement.prototype, key as "clientWidth", "get").mockReturnValue(value);
    onTestFinished(() => mock.mockRestore());
  }
}

async function settle(): Promise<void> {
  await act(async () => {});
  runOutTimers();
}

it.each([[300, 100], [100, 300], [300, 300]])("makes overflowing diagnostics keyboard reachable at %i by %i", async (width, height) => {
  layout(width, height);
  const { scroller } = mount();
  await settle();
  expect(scroller.getAttribute("tabindex")).toBe("0");
  expect(scroller.getAttribute("role")).toBe("region");
  expect(scroller.getAttribute("aria-label")).toBe("Error details");
});

it("does not add a scrolling tab stop when the text fits", async () => {
  layout(100, 100);
  const { scroller } = mount({ text: "short" });
  await settle();
  expect(scroller.hasAttribute("tabindex")).toBe(false);
  expect(scroller.hasAttribute("role")).toBe(false);
});

it.each([
  { disclosure: "summary", expected: "summary" },
  { disclosure: "summary", labelledBy: "heading", expected: "heading" },
])("keeps the diagnostic region named by $expected", async ({ expected, ...labels }) => {
  layout(300, 100);
  const { scroller } = mount(labels);
  await settle();
  expect(scroller.getAttribute("aria-labelledby")).toBe(expected);
  expect(scroller.hasAttribute("aria-label")).toBe(false);
});

/** A block whose clipboard refuses, with the failure alert's title that comes back; its message is the clipboard's. */
async function copyThatFails(failedText?: string): Promise<string> {
  fakeTimers();
  Object.assign(document, { execCommand: vi.fn(() => false) });
  const error = vi.fn();
  const container = mountForTooltip(
    <ViewerHost clipboard={{ writeText: async () => Promise.reject(new Error("denied")) }} notifications={{ notice: vi.fn(), error }}>
      <DiagnosticBlock failedText={failedText}>{TEXT}</DiagnosticBlock>
    </ViewerHost>,
  );
  activate(container.querySelector<HTMLButtonElement>('button[aria-label="Copy"]')!);
  await act(async () => {});
  expect(error).toHaveBeenCalledExactlyOnceWith("The clipboard could not be written to.");
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  return errorAlerts.getSnapshot()[0]!.title as string;
}

it("titles its alert as not copying the error details when its copy fails", async () => {
  expect(await copyThatFails()).toBe("Could not copy the error details");
});

it("titles its alert with what its caller gives, not error details, when its copy fails", async () => {
  expect(await copyThatFails("Could not copy the text")).toBe("Could not copy the text");
});
