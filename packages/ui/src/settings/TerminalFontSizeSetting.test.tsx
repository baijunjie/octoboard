// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished } from "vitest";

import { press, tap } from "../components/hint/tooltipTestSupport";
import { LanguageProvider } from "../i18n/react";
import { fakeTimers } from "../test/clock";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { resetTerminalFontSize, terminalFontSize } from "../terminal/terminalFontSize";
import { TerminalFontSizeSetting } from "./TerminalFontSizeSetting";
import { SettingsFocusContext } from "./useSectionRefocus";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** The row mounted with a stand-in for the settings dialog's selected section tab, which the context hands out. */
function mount(): { container: HTMLElement; sectionTab: HTMLButtonElement; input: HTMLInputElement; reset: HTMLButtonElement } {
  onTestFinished(resetTerminalFontSize);
  terminalFontSize.set(20);
  const sectionTab = document.body.appendChild(document.createElement("button"));
  onTestFinished(() => sectionTab.remove());
  const container = mountHost(
    <LanguageProvider>
      <SettingsFocusContext.Provider value={() => sectionTab}>
        <TerminalFontSizeSetting />
      </SettingsFocusContext.Provider>
    </LanguageProvider>,
    { daemon: testDaemon({ state: { connectionState: "open" } }) },
  );
  return {
    container,
    sectionTab,
    input: container.querySelector<HTMLInputElement>("input")!,
    reset: container.querySelector<HTMLButtonElement>('button[aria-label="Reset font size"]')!,
  };
}

it("moves focus to the number field's input after the reset button is pressed with a mouse", () => {
  const { sectionTab, input, reset } = mount();
  act(() => reset.focus());
  press(reset, "mouse");
  expect(terminalFontSize.get()).toBe(15);
  expect(document.activeElement).toBe(input);
  expect(document.activeElement).not.toBe(sectionTab);
});

it("moves focus to the number field's input after the reset button is pressed with the keyboard", () => {
  const { sectionTab, input, reset } = mount();
  act(() => reset.focus());
  tap(reset, "Enter");
  expect(terminalFontSize.get()).toBe(15);
  expect(document.activeElement).toBe(input);
  expect(document.activeElement).not.toBe(sectionTab);
});

it("moves focus to the selected section tab, never the input, after the reset button is tapped", () => {
  fakeTimers();
  const { sectionTab, input, reset } = mount();
  act(() => reset.focus());
  press(reset, "touch");
  expect(terminalFontSize.get()).toBe(15);
  expect(document.activeElement).toBe(sectionTab);
  expect(document.activeElement).not.toBe(input);
});

it("goes back to the input when the reset button is pressed with a mouse after a tap", () => {
  fakeTimers();
  const { input, reset } = mount();
  press(reset, "touch");
  act(() => terminalFontSize.set(20));
  press(reset, "mouse");
  expect(document.activeElement).toBe(input);
});
