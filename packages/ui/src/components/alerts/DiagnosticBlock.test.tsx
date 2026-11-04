// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { LanguageProvider } from "../../i18n/react";
import { PlatformProvider } from "../../platform/react";
import { activate, mountForTooltip } from "../hint/tooltipTestSupport";
import { DiagnosticBlock, DiagnosticLabelContext } from "./DiagnosticBlock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const TEXT = "  first line\r\n\tindented line\n" + "a long message ".repeat(100) + "\n\n";

function mount({ text = TEXT, labelledBy, disclosure }: { text?: string; labelledBy?: string; disclosure?: string } = {}) {
  const writeText = vi.fn(async (_text: string) => {});
  // No daemon: diagnostics must still work when startup fails before a connection exists.
  const container = mountForTooltip(
    <PlatformProvider value={{ kind: "browser", clipboard: { writeText } }}>
      <LanguageProvider>
        <DiagnosticLabelContext.Provider value={disclosure}>
          <DiagnosticBlock labelledBy={labelledBy}>{text}</DiagnosticBlock>
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

function layout(width: number, height: number): void {
  for (const [key, value] of Object.entries({ clientWidth: 100, clientHeight: 100, scrollWidth: width, scrollHeight: height })) {
    const mock = vi.spyOn(HTMLElement.prototype, key as "clientWidth", "get").mockReturnValue(value);
    onTestFinished(() => mock.mockRestore());
  }
}

async function settle(): Promise<void> {
  await act(() => new Promise<void>((resolve) => requestAnimationFrame(() => resolve())));
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
