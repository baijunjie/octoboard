// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { BareButton, Button, buttonClassName, type ButtonPress } from "./Button";
import { mount, pressWithMouse } from "./testSupport";
import { ToggleButton, ToggleButtonGroup } from "./ToggleButton";

function pressWithKeyboard(element: HTMLElement): void {
  act(() => {
    element.focus();
    element.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
    element.dispatchEvent(new KeyboardEvent("keyup", { key: "Enter", bubbles: true, cancelable: true }));
  });
}

it("maps the project's props onto the button's attributes", () => {
  const { container } = mount(
    <Button label="Close" describedBy="" expanded controls="panel" current iconOnly size="xs" variant="ghost" data-mark="" className="extra">
      x
    </Button>,
  );
  const button = container.querySelector("button")!;
  expect(button.getAttribute("type")).toBe("button");
  expect(button.getAttribute("aria-label")).toBe("Close");
  expect(button.getAttribute("aria-describedby")).toBe("");
  expect(button.getAttribute("aria-expanded")).toBe("true");
  expect(button.getAttribute("aria-controls")).toBe("panel");
  expect(button.getAttribute("aria-current")).toBe("true");
  expect(button.hasAttribute("data-mark")).toBe(true);
  expect(button.getAttribute("data-size")).toBe("xs");
  expect(button.className).toContain("app-button--xs");
  expect(button.className).toContain("button--sm");
  expect(button.className).toContain("button--icon-only");
  expect(button.className).toContain("button--ghost");
  expect(button.className).toContain("extra");
});

it("stays focusable and pressable while unavailable, describing itself with the reason and keeping its fill", () => {
  const onPress = vi.fn();
  const { container } = mount(<Button label="Keep" describedBy="other" variant="secondary" unavailable="Cannot be kept" onPress={onPress}>Keep</Button>);
  const button = container.querySelector("button")!;
  expect(button.getAttribute("aria-disabled")).toBe("true");
  expect(button.disabled).toBe(false);
  expect(button.getAttribute("aria-label")).toBe("Keep");
  const description = document.getElementById(button.getAttribute("aria-describedby")!)!;
  expect(description.textContent).toBe("Cannot be kept");
  expect(description.className).toBe("sr-only");
  expect(button.contains(description)).toBe(true);
  expect(button.className).toContain("pointer-events-auto");
  expect(button.className).toContain("[--button-bg-hover:var(--button-bg)]");
  pressWithKeyboard(button);
  expect(onPress).toHaveBeenCalledTimes(1);
});

it("shows a decorative spinner before the unchanged label while pending", () => {
  const { container } = mount(<Button pending>Save</Button>);
  const button = container.querySelector("button")!;
  const spinner = button.querySelector("[data-slot=spinner]")!;
  expect(button.textContent).toBe("Save");
  expect(spinner.getAttribute("aria-hidden")).toBe("true");
  expect(button.firstElementChild).toBe(spinner);
  expect(button.getAttribute("aria-busy")).toBe("true");
  expect(button.classList.contains("[--disabled-opacity:1]")).toBe(false);
});

it("shows neither spinner nor busy state when not pending", () => {
  const { container } = mount(<Button>Save</Button>);
  const button = container.querySelector("button")!;
  expect(button.querySelector("[data-slot=spinner]")).toBeNull();
  expect(button.hasAttribute("aria-busy")).toBe(false);
});

it("lets a pending icon-only button show the spinner in place of its icon", () => {
  const { container } = mount(<Button iconOnly pending label="Pick"><span data-testid="icon" /></Button>);
  const button = container.querySelector("button")!;
  expect(button.querySelector("[data-testid=icon]")).toBeNull();
  expect(button.querySelector("[data-slot=spinner]")).not.toBeNull();
});

it.each([false, true])("uses a custom indicator and opts out of dimming only while pending (icon-only: %s)", (iconOnly) => {
  const onPress = vi.fn();
  const view = (pending: boolean) => (
    <Button iconOnly={iconOnly} pending={pending} dimWhilePending={false} pendingIndicator={<span aria-hidden="true" data-testid="pending-indicator" />} label="Refresh" onPress={onPress}>
      <span data-testid="content">Refresh</span>
    </Button>
  );
  const { container, render } = mount(view(false));
  const button = container.querySelector("button")!;
  expect(button.querySelector("[data-testid=pending-indicator]")).toBeNull();
  expect(button.querySelector("[data-testid=content]")).not.toBeNull();
  expect(button.classList.contains("[--disabled-opacity:1]")).toBe(false);
  render(view(true));
  expect(button.querySelector("[data-testid=pending-indicator]")).not.toBeNull();
  expect(button.querySelector("[data-slot=spinner]")).toBeNull();
  expect(button.querySelector("[data-testid=content]") === null).toBe(iconOnly);
  expect(button.getAttribute("aria-busy")).toBe("true");
  expect(button.classList.contains("[--disabled-opacity:1]")).toBe(true);
  pressWithKeyboard(button);
  expect(onPress).not.toHaveBeenCalled();
  render(view(false));
  expect(button.querySelector("[data-testid=pending-indicator]")).toBeNull();
  expect(button.querySelector("[data-testid=content]")).not.toBeNull();
  expect(button.classList.contains("[--disabled-opacity:1]")).toBe(false);
  expect(button.getAttribute("aria-busy")).toBeNull();
  pressWithKeyboard(button);
  expect(onPress).toHaveBeenCalledTimes(1);
});

it("marks an autofocused button as its dialog's opening focus", () => {
  const { container } = mount(<><Button autoFocus>Save</Button><Button>Cancel</Button></>);
  const [save, cancel] = Array.from(container.querySelectorAll("button"));
  expect(save.hasAttribute("data-opening-focus")).toBe(true);
  expect(cancel.hasAttribute("data-opening-focus")).toBe(false);
});

it("tells onPress what pressed it, and keeps focus where it was when asked to", () => {
  const presses: ButtonPress["pointerType"][] = [];
  const { container } = mount(
    <>
      <input aria-label="Elsewhere" />
      <Button focusOnPress={false} onPress={(press) => presses.push(press.pointerType)}>Act</Button>
    </>,
  );
  const input = container.querySelector("input")!;
  const button = container.querySelector("button")!;
  input.focus();
  pressWithMouse(button);
  expect(presses).toEqual(["mouse"]);
  expect(document.activeElement).toBe(input);
  pressWithKeyboard(button);
  expect(presses).toEqual(["mouse", "keyboard"]);
});

it("reports only a mouse's hover", () => {
  const hovers: boolean[] = [];
  const { container } = mount(<Button onMouseHoverChange={(hovered) => hovers.push(hovered)}>Act</Button>);
  const button = container.querySelector("button")!;
  for (const pointerType of ["touch", "mouse"]) {
    act(() => {
      button.dispatchEvent(new PointerEvent("pointerover", { bubbles: true, pointerType }));
      button.dispatchEvent(new PointerEvent("pointerenter", { pointerType }));
      button.dispatchEvent(new PointerEvent("pointerout", { bubbles: true, pointerType }));
      button.dispatchEvent(new PointerEvent("pointerleave", { pointerType }));
    });
  }
  expect(hovers).toEqual([true, false]);
});

it("leaves a bare button without HeroUI's look and out of the tab order when asked", () => {
  const { container } = mount(<BareButton tabStop={false} expanded={false} className="band">Section</BareButton>);
  const button = container.querySelector("button")!;
  expect(button.className).toBe("band");
  expect(button.getAttribute("tabindex")).toBe("-1");
  expect(button.getAttribute("aria-expanded")).toBe("false");
});

it("draws the extra-small size for an element that borrows the button's look", () => {
  expect(buttonClassName({ variant: "secondary", size: "xs" })).toContain("app-button--xs");
  expect(buttonClassName({ variant: "secondary", size: "xs" })).toContain("button--sm");
  expect(buttonClassName({ size: "md" })).not.toContain("app-button--xs");
});

it("keeps exactly one option of a toggle group on, reported by its key", () => {
  const changes: string[] = [];
  const { container } = mount(
    <ToggleButtonGroup label="Theme" defaultSelectedKey="light" onSelectionChange={(key) => changes.push(key)}>
      <ToggleButton id="light">Light</ToggleButton>
      <ToggleButton id="dark">Dark</ToggleButton>
    </ToggleButtonGroup>,
  );
  const [light, dark] = Array.from(container.querySelectorAll("button"));
  pressWithKeyboard(light);
  expect(light.getAttribute("aria-checked")).toBe("true");
  pressWithKeyboard(dark);
  expect(changes.at(-1)).toBe("dark");
  expect(dark.getAttribute("aria-checked")).toBe("true");
  expect(light.getAttribute("aria-checked")).toBe("false");
});

it("reports a lone toggle's new state", () => {
  const states: boolean[] = [];
  const { container } = mount(<ToggleButton label="Match case" selected={false} onSelectedChange={(selected) => states.push(selected)}>Aa</ToggleButton>);
  pressWithKeyboard(container.querySelector("button")!);
  expect(states).toEqual([true]);
});
