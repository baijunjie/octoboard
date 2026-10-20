// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

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
