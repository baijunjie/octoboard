// @vitest-environment jsdom
import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { expect, it } from "vitest";

import { TextInput } from "./TextInput";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it("hands its form the trimmed value, and shows what was typed until it loses focus", () => {
  let held = "";
  const Form = () => {
    const [value, setValue] = useState("");
    held = value;
    return <TextInput label="Name" value={value} onChange={setValue} />;
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<Form />));
  const field = container.querySelector("input")!;

  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(field, "  my site ");
    field.dispatchEvent(new Event("input", { bubbles: true }));
  });
  expect(held).toBe("my site");
  expect(field.value).toBe("  my site ");

  act(() => field.dispatchEvent(new FocusEvent("focusout", { bubbles: true })));
  expect(field.value).toBe("my site");
  act(() => root.unmount());
});

it("shows a value set from outside as it is, surrounding whitespace included", () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<TextInput label="Name" value="My Folder " onChange={() => {}} />));
  expect(container.querySelector("input")!.value).toBe("My Folder ");
  act(() => root.unmount());
});
