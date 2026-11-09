// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { Label } from "./Field";
import { ListBox, ListBoxItem } from "./ListBox";
import { Select, SelectPopover, SelectTrigger } from "./Select";
import { mount, pressWithMouse } from "./testSupport";

function Field({ loading = false, disabled = false, invalid = false, onChange = () => {} }: { loading?: boolean; disabled?: boolean; invalid?: boolean; onChange?: (key: string | null) => void }) {
  return (
    <Select value="saved" onChange={onChange} loading={loading} disabled={disabled} invalid={invalid}>
      <Label>Model</Label>
      <SelectTrigger value="Saved model" />
      <SelectPopover>
        <ListBox>
          <ListBoxItem id="saved" textValue="Saved model">Saved model</ListBoxItem>
          <ListBoxItem id="other" textValue="Other model">Other model</ListBoxItem>
          <ListBoxItem id="disabled" textValue="Unavailable" disabled>Unavailable</ListBoxItem>
        </ListBox>
      </SelectPopover>
    </Select>
  );
}

it("substitutes a decorative spinner for the indicator and exposes busy state while options load", () => {
  const { container, render } = mount(<Field loading />);
  const trigger = container.querySelector<HTMLButtonElement>("[data-slot=select-trigger]")!;
  expect(trigger.disabled).toBe(true);
  expect(trigger.textContent).toBe("Saved model");
  expect(trigger.getAttribute("aria-busy")).toBe("true");
  const spinner = trigger.querySelector("[data-slot=spinner]")!;
  expect(spinner.getAttribute("aria-hidden")).toBe("true");
  expect(trigger.querySelector("[data-slot=select-default-indicator]")).toBeNull();
  render(<Field />);
  expect(trigger.disabled).toBe(false);
  expect(trigger.hasAttribute("aria-busy")).toBe(false);
  expect(trigger.querySelector("[data-slot=spinner]")).toBeNull();
  expect(trigger.querySelector("[data-slot=select-default-indicator]")).not.toBeNull();
  render(<Field disabled />);
  expect(trigger.disabled).toBe(true);
  expect(trigger.hasAttribute("aria-busy")).toBe(false);
});

it("keeps option collections usable and reports the selected option's string key", async () => {
  const changed = vi.fn();
  const { container } = mount(<Field onChange={changed} />);
  await act(async () => container.querySelector<HTMLButtonElement>("[data-slot=select-trigger]")!.click());
  const options = [...document.querySelectorAll<HTMLElement>("[role=option]")];
  expect(options.map((option) => option.textContent)).toEqual(["Saved model", "Other model", "Unavailable"]);
  expect(options[2]!.getAttribute("aria-disabled")).toBe("true");
  pressWithMouse(options[1]!);
  expect(changed).toHaveBeenCalledExactlyOnceWith("other");
});

it("leaves an invalid select out of native constraint validation", () => {
  const { container } = mount(<Field invalid />);
  expect(container.querySelector<HTMLSelectElement>("select")!.validity.valid).toBe(true);
});
