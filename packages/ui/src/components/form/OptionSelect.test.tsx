// @vitest-environment jsdom
import { act, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { expect, it, onTestFinished, vi } from "vitest";

import { AnchoredTooltip } from "../hint/AnchoredTooltip";
import { InlineReference } from "../../i18n/InlineReference";

import { currentLanguage, setLanguage } from "../../i18n/language";
import { fakeTimers, runOutTimers } from "../../test/clock";

import { OptionSelect } from "./OptionSelect";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it("keeps native option text raw when visible labels need a directed presentation", async () => {
  const label = "Task 中文｜مراجعة Work";
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<OptionSelect label="Owner" value="owner" onChange={() => {}} options={[
    { value: "owner", label, labelContent: <><bdi>Task 中文</bdi>｜<bdi>مراجعة Work</bdi></> },
  ]} />));
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  expect(container.querySelector('option[value="owner"]')?.textContent).toBe(label);
  expect(container.querySelector('option[value="owner"]')?.textContent).not.toMatch(/[\u2066-\u2069]/);
  expect(container.querySelector("button")?.textContent).toBe(label);
  expect(container.querySelectorAll("button bdi")).toHaveLength(2);
});

it("keeps badge text in native choices without changing raw selected keys or rich display", () => {
  const key = "model/中文-مراجعة";
  const label = "Model 中文 / مراجعة";
  const badge = "Not in the list";
  const changed = vi.fn();
  function Field() {
    const [value, setValue] = useState("listed");
    return <OptionSelect label="Model" value={value} onChange={(next) => { changed(next); setValue(next); }} options={[
      { value: "listed", label: "Listed" },
      { value: key, label, badge, labelContent: <InlineReference dir="auto" wrap>{label}</InlineReference> },
    ]} />;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  act(() => root.render(<Field />));
  const select = container.querySelector("select")!;
  const native = [...select.options].find((option) => option.value === key)!;
  expect(native.textContent).toBe(`${label}, ${badge}`);
  expect(native.outerHTML).not.toMatch(/[\u2066-\u2069]/);
  act(() => {
    select.value = key;
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
  expect(changed).toHaveBeenCalledExactlyOnceWith(key);
  expect(select.value).toBe(key);
  const trigger = container.querySelector("button")!;
  expect(trigger.querySelector("[data-inline-reference]")?.textContent).toBe(label);
  expect(trigger.textContent).toContain(`${label}, ${badge}`);
});

it("does not focus the trigger when the mark that holds focus goes in the render that disables it", async () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  const render = (marked: boolean, locked: boolean) => act(async () => root.render(
    <OptionSelect label="Model" value="a" onChange={() => {}} options={[{ value: "a", label: "A" }]} isDisabled={locked}
      mark={marked ? (markId) => <button type="button" data-status-mark-id={markId}>Why</button> : undefined} />,
  ));
  await render(true, false);
  const trigger = container.querySelector<HTMLButtonElement>("[data-slot=select-trigger]")!;
  const focusMark = () => act(() => container.querySelector<HTMLButtonElement>("[data-status-mark-id]")!.focus());
  focusMark();
  await render(false, true);
  expect(trigger.disabled).toBe(true);
  expect(document.activeElement).not.toBe(trigger);
  expect(document.activeElement).toBe(document.body);
  // Enabled, the trigger is handed focus.
  await render(true, false);
  focusMark();
  await render(false, false);
  expect(document.activeElement).toBe(trigger);
});

it("turns the focus ring on only when it hands focus to the trigger, not when the whole field goes with the mark", async () => {
  const modality = getInteractionModality();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    act(() => Reflect.apply(setInteractionModality, undefined, [modality]));
  });
  // The last input was a key, whose ring the handoff follows; each mark is focused after the modality is put back to
  // the pointer's.
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
  const render = (shown: boolean, marked: boolean) => act(async () => root.render(shown ? (
    <OptionSelect label="Model" value="a" onChange={() => {}} options={[{ value: "a", label: "A" }]}
      mark={marked ? (markId) => <button type="button" data-status-mark-id={markId}>Why</button> : undefined} />
  ) : null));
  const focusMarkAfterPointer = () => act(() => {
    setInteractionModality("pointer");
    container.querySelector<HTMLButtonElement>("[data-status-mark-id]")!.focus();
  });
  // The mark goes on its own: focus moves to the trigger, with the ring.
  await render(true, true);
  focusMarkAfterPointer();
  await render(true, false);
  expect(document.activeElement).toBe(container.querySelector("[data-slot=select-trigger]"));
  expect(getInteractionModality()).toBe("keyboard");
  // The field goes with it, as when its dialog closes: nothing is moved, and the modality is left as it was.
  await render(true, true);
  focusMarkAfterPointer();
  await render(false, false);
  expect(document.activeElement).toBe(document.body);
  expect(getInteractionModality()).toBe("pointer");
});

it("puts a label mark beside the label, keeps it out of the field's name, and hands focus to the trigger when it goes", async () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  const render = (marked: boolean) => act(async () => root.render(
    <OptionSelect label="Model" value="a" onChange={() => {}} options={[{ value: "a", label: "A" }]}
      labelMark={marked ? (markId) => <button type="button" data-status-mark-id={markId}>Why</button> : undefined} />,
  ));
  await render(true);
  const trigger = container.querySelector<HTMLButtonElement>("[data-slot=select-trigger]")!;
  const label = container.querySelector<HTMLElement>("[data-slot=label]")!;
  const mark = container.querySelector<HTMLButtonElement>("[data-status-mark-id]")!;
  expect(label.contains(mark)).toBe(false);
  expect(label.compareDocumentPosition(mark) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(document.getElementById(trigger.getAttribute("aria-labelledby")!.split(" ").at(-1)!)).toBe(label);
  act(() => mark.focus());
  await render(false);
  expect(document.activeElement).toBe(trigger);
});

it("marks the select invalid and describes it by its error, which stands under it as an alert", () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  const render = (errorMessage?: string) => act(() => root.render(
    <OptionSelect label="Account" value="a" onChange={() => {}} options={[{ value: "a", label: "Work" }]} errorMessage={errorMessage} />,
  ));
  render();
  const trigger = () => container.querySelector<HTMLElement>('[data-slot="select-trigger"]')!;
  expect(trigger().getAttribute("aria-invalid")).not.toBe("true");
  expect(container.querySelector('[role="alert"]')).toBeNull();
  render("Account Work is disabled.");
  const alert = container.querySelector('[role="alert"]');
  expect(alert?.textContent).toBe("Account Work is disabled.");
  expect(trigger().getAttribute("aria-describedby")?.split(" ")).toContain(alert!.closest('[data-slot="field-error"]')!.id);
  render();
  expect(container.querySelector('[role="alert"]')).toBeNull();
});

it("dims a disabled option's name by colour and its icon by fade, and announces it, whether or not it gives a reason", async () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  const icon = <svg data-testid="icon" />;
  act(() => root.render(<OptionSelect label="Account" value="a" onChange={() => {}} options={[
    { value: "a", label: "Free", icon },
    { value: "b", label: "Busy", icon, isDisabled: true },
    { value: "c", label: "Used up", icon, isDisabled: true, detail: "Limit reached", reason: { state: "Limit reached" } },
  ]} />));
  await act(async () => container.querySelector<HTMLElement>('[data-slot="select-trigger"]')!.click());
  const option = (name: string) => [...document.body.querySelectorAll<HTMLElement>('[role="option"]')].find((item) => item.textContent?.startsWith(name))!;
  expect(option("Free").getAttribute("aria-disabled")).toBeNull();
  expect(option("Free").querySelector('[class*="text-app-muted"]')).toBeNull();
  expect(option("Free").querySelector('[class*="opacity-(--disabled-opacity)"]')).toBeNull();
  for (const name of ["Busy", "Used up"]) {
    expect(option(name).getAttribute("aria-disabled")).toBe("true");
    // The icon fades; the name takes the muted colour and is never faded by opacity.
    expect(option(name).querySelector('[class*="opacity-(--disabled-opacity)"]')?.querySelector("svg")).not.toBeNull();
    const nameBox = [...option(name).querySelectorAll<HTMLElement>('[class*="text-app-muted"]')].find((el) => el.textContent?.startsWith(name))!;
    expect(nameBox.className).not.toContain("opacity-");
    expect(option(name).className).not.toContain("opacity-100");
  }
  expect(option("Busy").textContent).toBe("Busy");
});

it("names an option by its label and badge alone, describes it by its detail, and draws label and badge in one row", async () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  act(() => root.render(<OptionSelect label="Model" value="a" onChange={() => {}} options={[
    { value: "a", label: "Opus", detail: "Opus 5.5", reason: { state: "Opus 5.5" } },
    { value: "b", label: "Fable", badge: "Unavailable", detail: "Fable 5.1", reason: { state: "Fable 5.1" } },
  ]} />));
  await act(async () => container.querySelector<HTMLElement>('[data-slot="select-trigger"]')!.click());
  const option = (name: string) => [...document.body.querySelectorAll<HTMLElement>('[role="option"]')].find((item) => item.textContent?.startsWith(name))!;
  const text = (item: HTMLElement, attribute: string) =>
    (item.getAttribute(attribute) ?? "").split(" ").map((id) => document.getElementById(id)?.textContent ?? "").join(" ");
  expect(text(option("Opus"), "aria-labelledby")).toBe("Opus");
  expect(text(option("Opus"), "aria-describedby")).toBe("Opus 5.5");
  // The badge is part of the name, not of the description.
  expect(text(option("Fable"), "aria-labelledby")).toContain("Unavailable");
  expect(text(option("Fable"), "aria-labelledby")).not.toContain("5.1");
  expect(text(option("Fable"), "aria-describedby")).toBe("Fable 5.1");
  // One row holds the label and the badge, with the version line below it.
  const name = document.getElementById(option("Fable").getAttribute("aria-labelledby")!.split(" ")[0]!)!;
  expect(name.textContent).toContain("Fable");
  expect(name.textContent).toContain("Unavailable");
  expect(name.parentElement!.className).toContain("flex-col");
});

it("keeps aria-disabled in step with an option that becomes disabled and enabled again", async () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  const render = (isDisabled: boolean) => act(() => root.render(<OptionSelect label="Account" value="a" onChange={() => {}} options={[
    { value: "a", label: "Free" },
    { value: "b", label: "Busy", isDisabled },
  ]} />));
  render(false);
  await act(async () => container.querySelector<HTMLElement>('[data-slot="select-trigger"]')!.click());
  const busy = () => [...document.body.querySelectorAll<HTMLElement>('[role="option"]')].find((item) => item.textContent?.startsWith("Busy"))!;
  expect(busy().getAttribute("aria-disabled")).toBeNull();
  render(true);
  expect(busy().getAttribute("aria-disabled")).toBe("true");
  render(false);
  expect(busy().getAttribute("aria-disabled")).toBeNull();
});

it("closes only an option's tooltip on Escape, leaving the list open with focus where it was, and the next Escape closes the list", async () => {
  /** A label whose tooltip shows as the list opens, as a clipped one's does under the pointer. */
  function Hovered() {
    const anchor = useRef<HTMLSpanElement>(null);
    return (
      <>
        <span ref={anchor}>Long</span>
        <AnchoredTooltip tooltip="The whole Long" anchor={anchor} hovered immediate />
      </>
    );
  }
  // react-aria hands focus to the option a frame after the list opens; the fake clock runs that frame out.
  fakeTimers();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  act(() => root.render(<OptionSelect label="Branch" value="a" onChange={() => {}} options={[
    { value: "a", label: "Short" },
    { value: "b", label: "Long", labelContent: <Hovered /> },
  ]} />));
  await act(async () => container.querySelector<HTMLElement>('[data-slot="select-trigger"]')!.click());
  runOutTimers();
  const tooltip = () => document.querySelector('[role="tooltip"]');
  const escape = () => act(async () => void document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));
  const focused = document.activeElement;
  expect(focused?.getAttribute("role")).toBe("option");
  expect(tooltip()?.textContent).toBe("The whole Long");

  await escape();
  expect(tooltip()).toBeNull();
  expect(document.querySelector('[role="listbox"]')).not.toBeNull();
  expect(document.activeElement).toBe(focused);

  await escape();
  expect(document.querySelector('[role="listbox"]')).toBeNull();
});


it("replaces an empty select's placeholder and badge with localized loading text and restores both when ready", () => {
  const language = currentLanguage();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); act(() => setLanguage(language)); });
  act(() => setLanguage("zh-Hans"));
  const render = (isLoading: boolean) => act(() => root.render(
    <OptionSelect label="Model" value={null} onChange={() => {}} options={[]} placeholder="Same as console" placeholderBadge="Inherited model" isLoading={isLoading} />,
  ));
  render(true);
  const trigger = container.querySelector<HTMLButtonElement>("[data-slot=select-trigger]")!;
  expect(trigger.textContent).toBe("加载中…");
  expect(trigger.disabled).toBe(true);
  expect(trigger.getAttribute("aria-busy")).toBe("true");
  expect(trigger.querySelector("[data-slot=spinner]")).not.toBeNull();
  expect(trigger.querySelector("[data-slot=select-default-indicator]")).toBeNull();
  expect(container.querySelector("[data-slot=description]")).toBeNull();
  render(false);
  expect(trigger.textContent).toContain("Same as console");
  expect(trigger.textContent).toContain("Inherited model");
  expect(trigger.disabled).toBe(false);
  expect(trigger.hasAttribute("aria-busy")).toBe(false);
  expect(trigger.querySelector("[data-slot=spinner]")).toBeNull();
  expect(trigger.querySelector("[data-slot=select-default-indicator]")).not.toBeNull();
});

it("preserves a stored choice while loading and blocks the clear button and clearing keys", () => {
  const changed = vi.fn();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  const render = (isLoading: boolean) => act(() => root.render(
    <OptionSelect label="Model" value="saved" onChange={changed} options={[{ value: "saved", label: "Saved model" }]} placeholder="Agent default" isLoading={isLoading} />,
  ));
  render(true);
  const trigger = container.querySelector<HTMLButtonElement>("[data-slot=select-trigger]")!;
  const clear = [...container.querySelectorAll<HTMLButtonElement>("button")].find((button) => button !== trigger)!;
  expect(trigger.textContent).toBe("Saved model");
  expect(trigger.disabled).toBe(true);
  expect(clear.disabled).toBe(true);
  act(() => {
    clear.click();
    for (const key of ["Backspace", "Delete"]) trigger.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
  });
  expect(changed).not.toHaveBeenCalled();
  expect(container.querySelector<HTMLSelectElement>("select")!.value).toBe("saved");
  render(false);
  expect(trigger.textContent).toBe("Saved model");
  expect(trigger.disabled).toBe(false);
  expect(clear.disabled).toBe(false);
});

it.each([[undefined, "secondary"], ["secondary", "secondary"], ["primary", "primary"]] as const)("draws the select as variant %s: %s", (variant, drawn) => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  act(() => root.render(<OptionSelect label="Owner" variant={variant} value="a" onChange={() => {}} options={[{ value: "a", label: "A" }]} />));
  const select = container.querySelector("[data-slot=select]")!;
  expect(select.classList.contains(`select--${drawn}`)).toBe(true);
  expect(select.classList.contains(`select--${drawn === "primary" ? "secondary" : "primary"}`)).toBe(false);
});

it("draws a locked, disabled select as a value to read, and an unlocked disabled one as a control", () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  const render = (locked: boolean) => act(() => root.render(<OptionSelect label="Owner" isDisabled locked={locked} value="a" onChange={() => {}} options={[{ value: "a", label: "A" }]} />));
  render(false);
  const trigger = container.querySelector<HTMLButtonElement>("[data-slot=select-trigger]")!;
  expect(trigger.disabled).toBe(true);
  expect(trigger.classList.contains("bg-transparent")).toBe(false);
  render(true);
  expect(trigger.disabled).toBe(true);
  expect(trigger.textContent).toBe("A");
  for (const name of ["border", "border-separator", "bg-transparent", "shadow-none"]) expect(trigger.classList.contains(name)).toBe(true);
});

it("gives the label and the description of a select the direction of their text when asked", () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  const render = (textDir?: "auto") => act(() => root.render(<OptionSelect label="Owner" description="Who" textDir={textDir} value="a" onChange={() => {}} options={[{ value: "a", label: "A" }]} />));
  render();
  expect(container.querySelector("[data-slot=label]")!.hasAttribute("dir")).toBe(false);
  render("auto");
  expect(container.querySelector("[data-slot=label]")!.getAttribute("dir")).toBe("auto");
  expect(container.querySelector("[data-slot=description]")!.getAttribute("dir")).toBe("auto");
});
