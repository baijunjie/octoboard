// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { act } from "react";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";


import { BareButton, Button, buttonClassName, type ButtonPress, FIELD_CONTROL_CLASS } from "./Button";
import { mount, pressWithMouse } from "./testSupport";
import { ToggleButton, ToggleButtonGroup } from "./ToggleButton";

beforeEach(() => {
  const style = document.head.appendChild(document.createElement("style"));
  style.textContent = readFileSync(join(import.meta.dirname, "Button.css"), "utf8");
  onTestFinished(() => style.remove());
});

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

it("stays focusable while unavailable, takes no press, describing itself with the reason and keeping its fill", () => {
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
  button.click();
  expect(onPress).not.toHaveBeenCalled();
});

it("overlays a decorative spinner while keeping the hidden label as its accessible name", () => {
  const { container } = mount(<Button pending>Save</Button>);
  const button = container.querySelector("button")!;
  const spinner = button.querySelector("[data-slot=spinner]")!;
  expect(button.textContent).toBe("Save");
  expect(spinner.getAttribute("aria-hidden")).toBe("true");
  const content = document.getElementById(button.getAttribute("aria-labelledby")!.split(" ")[0])!;
  expect(content).not.toBeNull();
  expect(content.textContent).toBe("Save");
  expect(getComputedStyle(content).display).toBe("contents");
  expect(getComputedStyle(content).visibility).toBe("hidden");
  expect(content.hasAttribute("aria-hidden")).toBe(false);
  const overlay = spinner.parentElement!;
  expect(overlay.getAttribute("aria-hidden")).toBe("true");
  expect(getComputedStyle(overlay).position).toBe("absolute");
  expect(getComputedStyle(overlay).inset).toBe("0px");
  expect(getComputedStyle(overlay).alignItems).toBe("center");
  expect(getComputedStyle(overlay).justifyContent).toBe("center");
  expect(button.getAttribute("aria-busy")).toBe("true");
  expect(button.classList.contains("[--disabled-opacity:1]")).toBe(false);
});

it("shows neither spinner nor busy state when not pending", () => {
  const { container } = mount(<Button>Save</Button>);
  const button = container.querySelector("button")!;
  expect(button.querySelector("[data-slot=spinner]")).toBeNull();
  expect(button.hasAttribute("aria-busy")).toBe(false);
});

it("keeps a pending icon-only button's icon hidden in place beneath its spinner", () => {
  const { container } = mount(<Button iconOnly pending label="Pick"><span data-testid="icon" /></Button>);
  const button = container.querySelector("button")!;
  const icon = button.querySelector("[data-testid=icon]")!;
  expect(icon).not.toBeNull();
  expect(getComputedStyle(icon.parentElement!).visibility).toBe("hidden");
  expect(button.getAttribute("aria-label")).toBe("Pick");
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
  act(() => button.focus());
  render(view(true));
  expect(document.activeElement).toBe(button);
  expect(button.querySelector("[data-testid=pending-indicator]")).not.toBeNull();
  expect(button.querySelector("[data-slot=spinner]")).toBeNull();
  expect(button.querySelector("[data-testid=content]")).not.toBeNull();
  expect(button.getAttribute("aria-busy")).toBe("true");
  expect(button.classList.contains("[--disabled-opacity:1]")).toBe(true);
  pressWithKeyboard(button);
  expect(onPress).not.toHaveBeenCalled();
  render(view(false));
  expect(button.querySelector("[data-testid=pending-indicator]")).toBeNull();
  expect(button.querySelector("[data-testid=content]")).not.toBeNull();
  expect(button.classList.contains("[--disabled-opacity:1]")).toBe(false);
  expect(getComputedStyle(button.querySelector("[data-testid=content]")!.parentElement!).visibility).toBe("visible");
  expect(button.getAttribute("aria-busy")).toBeNull();
  pressWithKeyboard(button);
  expect(onPress).toHaveBeenCalledTimes(1);
});

it("preserves explicit naming and the same content nodes through pending transitions", () => {
  const view = (pending: boolean) => (
    <>
      <span id="action-name">Save changes</span>
      <Button pending={pending} label="Save document"><span data-testid="label-content">Save</span></Button>
      <Button pending={pending} labelledBy="action-name" label="Ignored">Save</Button>
    </>
  );
  const { container, render } = mount(view(false));
  const content = container.querySelector("[data-testid=label-content]")!;
  const [labelled, referenced] = Array.from(container.querySelectorAll("button"));
  for (const pending of [true, false]) {
    render(view(pending));
    expect(labelled.getAttribute("aria-label")).toBe("Save document");
    expect(labelled.getAttribute("aria-labelledby")?.split(" ") ?? []).not.toContain(content.parentElement!.id);
    expect(referenced.getAttribute("aria-labelledby")!.split(" ")).toContain("action-name");
    expect(document.getElementById("action-name")!.textContent).toBe("Save changes");
    expect(container.querySelector("[data-testid=label-content]")).toBe(content);
    expect(getComputedStyle(content.parentElement!).visibility).toBe(pending ? "hidden" : "visible");
  }
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

it("gives an icon button and a text button beside a field the same field shape, keeping the caller's variant", () => {
  const icon = mount(<Button besideField iconOnly label="Paste" variant="secondary">p</Button>).container.querySelector("button")!.classList;
  const text = mount(<Button besideField variant="secondary">Browse</Button>).container.querySelector("button")!.classList;
  for (const classes of [icon, text]) {
    expect(classes).toContain(FIELD_CONTROL_CLASS);
    expect(classes).toContain("button--secondary");
  }
  expect(icon).toContain("button--icon-only");
  expect(text).not.toContain("button--icon-only");
});

it("leaves a button that is not beside a field without the field shape", () => {
  const { container } = mount(<Button iconOnly label="Close" variant="ghost">x</Button>);
  const classes = container.querySelector("button")!.classList;
  expect(classes).not.toContain(FIELD_CONTROL_CLASS);
  expect(classes).toContain("button--ghost");
});

it("gives a toggle beside a field the same field shape as a button, keeping the caller's variant", () => {
  const beside = mount(<ToggleButton besideField iconOnly variant="default" label="Include ignored" selected={false}>i</ToggleButton>).container.querySelector("button")!.classList;
  expect(beside).toContain(FIELD_CONTROL_CLASS);
  expect(beside).not.toContain("toggle-button--ghost");
  const plain = mount(<ToggleButton iconOnly variant="ghost" label="Match case" selected={false}>Aa</ToggleButton>).container.querySelector("button")!.classList;
  expect(plain).not.toContain(FIELD_CONTROL_CLASS);
  expect(plain).toContain("toggle-button--ghost");
});
