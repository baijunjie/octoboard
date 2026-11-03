// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { SettingRow, SettingSwitch } from "./SettingRow";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(element: React.ReactElement) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(element));
  return container;
}

it("puts the help at the end of the label's line, before the description", () => {
  const container = mount(<SettingRow label="Appearance" help={<button>About appearance</button>} description="Muted line" />);
  const line = container.querySelector("button")!.parentElement!.parentElement!;
  expect(line.textContent).toBe("AppearanceAbout appearance");
  expect(line.lastElementChild!.firstElementChild).toBe(container.querySelector("button"));
  expect(line.nextElementSibling?.textContent).toBe("Muted line");
});

it("renders no description line when it has none", () => {
  const container = mount(<SettingRow label="Appearance" help={<button>About appearance</button>} />);
  const line = container.querySelector("button")!.parentElement!;
  expect(line.nextElementSibling).toBeNull();
});

it("marks a switch the dialog around it opens on, and no other switch", () => {
  const container = mount(<SettingSwitch label="Share" isSelected={false} onChange={() => {}} opensFocused />);
  const input = container.querySelector("input")!;
  expect(input.hasAttribute("data-opening-focus")).toBe(true);
  expect(mount(<SettingSwitch label="Share" isSelected={false} onChange={() => {}} />).querySelector("input")!.hasAttribute("data-opening-focus")).toBe(false);
});

it("ignores a press while the save the switch's last press started is unanswered, and takes one again once it settles", async () => {
  const saves: { settle: () => void; fail: (error: Error) => void }[] = [];
  const onChange = vi.fn(() => new Promise<void>((settle, fail) => { saves.push({ settle, fail }); }));
  const input = mount(<SettingSwitch label="Share" isSelected={false} onChange={onChange} />).querySelector("input")!;
  act(() => input.click());
  act(() => input.click());
  expect(onChange).toHaveBeenCalledTimes(1);
  // The guard is not shown: the switch stays as available as before.
  expect(input.hasAttribute("aria-disabled")).toBe(false);
  await act(async () => saves[0]!.fail(new Error("refused")));
  act(() => input.click());
  expect(onChange).toHaveBeenCalledTimes(2);
  await act(async () => saves[1]!.settle());
  act(() => input.click());
  expect(onChange).toHaveBeenCalledTimes(3);
});

it("takes every press when the change starts no save", () => {
  const onChange = vi.fn();
  const input = mount(<SettingSwitch label="Share" isSelected={false} onChange={onChange} />).querySelector("input")!;
  act(() => input.click());
  act(() => input.click());
  expect(onChange).toHaveBeenCalledTimes(2);
});
