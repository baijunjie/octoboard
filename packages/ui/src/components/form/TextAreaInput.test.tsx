// @vitest-environment jsdom
import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";

import { TextAreaInput } from "./TextAreaInput";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(ui: React.ReactElement) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(ui));
  return { container, unmount: () => act(() => root.unmount()) };
}

it("hands its form what is typed untrimmed, and tells it when the field is left", () => {
  const left = vi.fn();
  let held = "";
  const Form = () => {
    const [value, setValue] = useState("");
    held = value;
    return <TextAreaInput label="Prompt" value={value} onChange={setValue} onBlur={left} />;
  };
  const { container, unmount } = mount(<Form />);
  const field = container.querySelector("textarea")!;
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!.call(field, "  first\nsecond  ");
    field.dispatchEvent(new Event("input", { bubbles: true }));
  });
  expect(held).toBe("  first\nsecond  ");
  expect(left).not.toHaveBeenCalled();
  act(() => field.focus());
  act(() => field.blur());
  expect(left).toHaveBeenCalledOnce();
  unmount();
});

it("marks the field invalid and shows the error only while there is one", () => {
  const { container, unmount } = mount(<TextAreaInput label="Prompt" value="" onChange={() => {}} errorMessage="Enter a prompt." />);
  expect(container.querySelector("textarea")!.getAttribute("aria-invalid")).toBe("true");
  expect(container.textContent).toContain("Enter a prompt.");
  unmount();
  const clean = mount(<TextAreaInput label="Prompt" value="" onChange={() => {}} />);
  expect(clean.container.querySelector("textarea")!.getAttribute("aria-invalid")).not.toBe("true");
  expect(clean.container.textContent).not.toContain("Enter a prompt.");
  clean.unmount();
});
