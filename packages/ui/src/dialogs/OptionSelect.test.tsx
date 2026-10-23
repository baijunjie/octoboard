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
