// @vitest-environment jsdom
import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { InlineReference } from "../components/InlineReference";

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
  // The hand-off decides after the commit, in a microtask, which `act` waits for only when awaited.
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
