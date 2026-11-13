// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { CONTROL_TOOLTIP_DELAY_MS, enter, openTooltip, press, tooltipsCooledDown } from "../components/hint/tooltipTestSupport";
import { LanguageProvider } from "../i18n/react";
import { ThemeProvider } from "../preferences/theme";
import { fakeTimers } from "../test/clock";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { resetTerminalFontSize, terminalFontSize } from "../terminal/terminalFontSize";
import { AppearanceSection } from "./AppearanceSection";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(): HTMLElement {
  // The theme provider asks the window for the system's colour scheme.
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: false, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  onTestFinished(() => void vi.unstubAllGlobals());
  return mountHost(
    <LanguageProvider>
      <ThemeProvider>
        <AppearanceSection />
      </ThemeProvider>
    </LanguageProvider>,
    { daemon: testDaemon({ state: { connectionState: "open" } }) },
  );
}

it("shows the theme row, the Markdown style row, then the terminal font size row, and no group", () => {
  const container = mount();
  expect([...container.querySelectorAll<HTMLElement>("[data-settings-search-id]")].map((row) => row.dataset.settingsSearchId)).toEqual(["theme", "markdown-style", "terminal-font-size"]);
  expect(container.querySelector("[data-settings-search-group]")).toBeNull();
});

it("shows the theme as Light, Dark and System choices and the Markdown style as a select", () => {
  const container = mount();
  const theme = container.querySelector('[data-settings-search-id="theme"]')!;
  expect([...theme.querySelectorAll('[role="radio"], button')].map((choice) => choice.getAttribute("aria-label") ?? choice.textContent)).toEqual(expect.arrayContaining(["Light", "Dark", "System"]));
  expect(container.querySelector('[data-settings-search-id="markdown-style"]')!.textContent).toContain("Markdown style");
});

it("shows no language, sorting or other General row", () => {
  const container = mount();
  expect(container.querySelector('[data-settings-search-id="language"]')).toBeNull();
  expect(container.querySelector('[data-settings-search-id^="session-order"]')).toBeNull();
});

it("shows the terminal font size as a number field named by the row, with the size and no unit", () => {
  onTestFinished(resetTerminalFontSize);
  const container = mount();
  const row = container.querySelector('[data-settings-search-id="terminal-font-size"]')!;
  const input = row.querySelector<HTMLInputElement>("input")!;
  expect(input.getAttribute("aria-label")).toBe("Terminal font size");
  expect(input.value).toBe("15");
  expect(input.hasAttribute("aria-describedby")).toBe(false);
  expect(row.textContent).not.toContain("px");
  expect(row.querySelector('input[type="range"]')).toBeNull();
  // The only other button in the row, besides the field's two steppers, is the reset button.
  expect([...row.querySelectorAll("button")].filter((button) => !button.hasAttribute("slot")).map((button) => button.getAttribute("aria-label"))).toEqual(["Reset font size"]);
});

it("changes the font size from the stepper buttons and the arrow keys, and disables the button at its bound", () => {
  onTestFinished(resetTerminalFontSize);
  const container = mount();
  const row = container.querySelector('[data-settings-search-id="terminal-font-size"]')!;
  const input = row.querySelector<HTMLInputElement>("input")!;
  const decrement = row.querySelector<HTMLButtonElement>('[slot="decrement"]')!;
  const increment = row.querySelector<HTMLButtonElement>('[slot="increment"]')!;
  const press = (key: string) => act(() => void input.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true })));
  act(() => increment.click());
  expect(terminalFontSize.get()).toBe(16);
  expect(input.value).toBe("16");
  act(() => decrement.click());
  expect(terminalFontSize.get()).toBe(15);
  press("ArrowUp");
  expect(terminalFontSize.get()).toBe(16);
  press("ArrowDown");
  expect(terminalFontSize.get()).toBe(15);
  press("End");
  expect(terminalFontSize.get()).toBe(24);
  expect(increment.disabled).toBe(true);
  expect(decrement.disabled).toBe(false);
  press("Home");
  expect(terminalFontSize.get()).toBe(10);
  expect(decrement.disabled).toBe(true);
  expect(increment.disabled).toBe(false);
});

it("takes a typed size on blur, held to the range", () => {
  onTestFinished(resetTerminalFontSize);
  const container = mount();
  const input = container.querySelector<HTMLInputElement>('[data-settings-search-id="terminal-font-size"] input')!;
  const type = (text: string) => {
    act(() => {
      input.focus();
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, text);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
    act(() => input.blur());
  };
  type("18");
  expect(terminalFontSize.get()).toBe(18);
  type("99");
  expect(terminalFontSize.get()).toBe(24);
  expect(input.value).toBe("24");
  type("2");
  expect(terminalFontSize.get()).toBe(10);
});

const resetButton = (container: HTMLElement) => container.querySelector<HTMLButtonElement>('[data-settings-search-id="terminal-font-size"] button[aria-label="Reset font size"]')!;
const fontSizeInput = (container: HTMLElement) => container.querySelector<HTMLInputElement>('[data-settings-search-id="terminal-font-size"] input')!;

it("puts the reset button before the number field in the row, and keeps it a plain enabled, visible button while the size is not the default", () => {
  onTestFinished(resetTerminalFontSize);
  terminalFontSize.set(20);
  const container = mount();
  const button = resetButton(container);
  expect(button).not.toBeNull();
  expect(fontSizeInput(container).compareDocumentPosition(button) & Node.DOCUMENT_POSITION_PRECEDING).toBeTruthy();
  expect(button.disabled).toBe(false);
  expect(button.getAttribute("aria-disabled")).toBeNull();
  expect(button.textContent).toBe("");
  expect(button.closest(".invisible")).toBeNull();
  expect(button.closest("[aria-hidden=true]")).toBeNull();
  act(() => button.focus());
  expect(document.activeElement).toBe(button);
});

it("hides the reset button at the default size but keeps its place: out of the accessibility tree, disabled and not focusable", () => {
  onTestFinished(resetTerminalFontSize);
  const container = mount();
  const button = resetButton(container);
  expect(button).not.toBeNull();
  expect(fontSizeInput(container).compareDocumentPosition(button) & Node.DOCUMENT_POSITION_PRECEDING).toBeTruthy();
  expect(button.closest(".invisible")).not.toBeNull();
  expect(button.closest("[aria-hidden=true]")).not.toBeNull();
  expect(button.disabled).toBe(true);
  act(() => button.focus());
  expect(document.activeElement).not.toBe(button);
});

it("does nothing when the hidden reset button is pressed at the default size", () => {
  onTestFinished(resetTerminalFontSize);
  const container = mount();
  press(resetButton(container), "mouse");
  expect(terminalFontSize.get()).toBe(15);
  expect(fontSizeInput(container).value).toBe("15");
});

it("sets the size back to the default when the reset button is pressed, and the field follows", () => {
  onTestFinished(resetTerminalFontSize);
  terminalFontSize.set(21);
  const container = mount();
  const input = fontSizeInput(container);
  expect(input.value).toBe("21");
  press(resetButton(container), "mouse");
  expect(terminalFontSize.get()).toBe(15);
  expect(input.value).toBe("15");
});

it("moves focus to the number field's input when the reset button is pressed, since the button then hides", () => {
  onTestFinished(resetTerminalFontSize);
  terminalFontSize.set(20);
  const container = mount();
  const button = resetButton(container);
  act(() => button.focus());
  expect(document.activeElement).toBe(button);
  press(button, "mouse");
  expect(document.activeElement).toBe(fontSizeInput(container));
  expect(button.closest(".invisible")).not.toBeNull();
});

it("shows the reset button as soon as the size leaves the default, and hides it again once reset", () => {
  onTestFinished(resetTerminalFontSize);
  const container = mount();
  const button = resetButton(container);
  const increment = container.querySelector<HTMLButtonElement>('[data-settings-search-id="terminal-font-size"] [slot="increment"]')!;
  expect(button.closest(".invisible")).not.toBeNull();
  act(() => increment.click());
  expect(terminalFontSize.get()).toBe(16);
  expect(button.closest(".invisible")).toBeNull();
  expect(button.closest("[aria-hidden=true]")).toBeNull();
  expect(button.disabled).toBe(false);
  expect(button.getAttribute("aria-label")).toBe("Reset font size");
  press(button, "mouse");
  expect(terminalFontSize.get()).toBe(15);
  expect(button.closest(".invisible")).not.toBeNull();
  expect(button.disabled).toBe(true);
});

it("shows the tooltip \"Reset font size\" on the reset button away from the default size", async () => {
  onTestFinished(resetTerminalFontSize);
  terminalFontSize.set(18);
  await tooltipsCooledDown();
  fakeTimers();
  const container = mount();
  enter(resetButton(container));
  act(() => void vi.advanceTimersByTime(CONTROL_TOOLTIP_DELAY_MS));
  expect(openTooltip()?.textContent).toBe("Reset font size");
});
