// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import { Button } from "./Button";
import { Checkbox } from "./Checkbox";
import { Label } from "./Field";
import { ListBox, ListBoxItem } from "./ListBox";
import { Switch } from "./Switch";
import { mount, pressWithMouse } from "./testSupport";
import { Input, SearchField, TextField } from "./TextField";

it("marks a text field's input as its dialog's opening focus when the field is given autoFocus", () => {
  const { container } = mount(
    <TextField value="" onChange={() => {}} autoFocus>
      <Label>Name</Label>
      <Input />
    </TextField>,
  );
  const input = container.querySelector("input")!;
  expect(input.hasAttribute("data-opening-focus")).toBe(true);
  expect(input.getAttribute("aria-labelledby")).toBeTruthy();
});

it("keeps a text field's value and invalid state on its input, and leaves the submit to the form", () => {
  const { container } = mount(
    <TextField value="abc" onChange={() => {}} invalid>
      <Label>Name</Label>
      <Input />
    </TextField>,
  );
  const input = container.querySelector("input")!;
  expect(input.value).toBe("abc");
  expect(input.getAttribute("aria-invalid")).toBe("true");
  expect(input.validity.valid).toBe(true);
});

it("keeps the input mounted while an end adornment comes and goes", () => {
  const { container, render } = mount(<Input label="Path" value="" onChange={() => {}} end={null} />);
  const input = container.querySelector("input");
  render(<Input label="Path" value="" onChange={() => {}} end={<span>…</span>} />);
  expect(container.querySelector("input")).toBe(input);
  expect(input?.getAttribute("aria-label")).toBe("Path");
});

it("hands a search field's clear button the field's clear action and keeps a trailing button its own", () => {
  const onChange = vi.fn();
  const onMark = vi.fn();
  const { container } = mount(
    <SearchField
      label="Filter"
      value="abc"
      onChange={onChange}
      trailing={<Button label="Mark" onPress={onMark} />}
      clearButton={<Button label="Clear" />}
    />,
  );
  pressWithMouse(container.querySelector<HTMLElement>('[aria-label="Mark"]')!);
  expect(onMark).toHaveBeenCalledOnce();
  expect(onChange).not.toHaveBeenCalled();
  const clear = container.querySelector<HTMLElement>('[aria-label="Clear"]')!;
  expect(clear.className).toContain("me-2 shrink-0");
  pressWithMouse(clear);
  expect(onChange).toHaveBeenCalledWith("");
});

it("keeps a search field's clear button out of the tab order", () => {
  const { container } = mount(<SearchField label="Filter" value="abc" onChange={() => {}} clearButton={<Button label="Clear" />} />);
  const clear = container.querySelector<HTMLElement>('[aria-label="Clear"]')!;
  expect(clear.getAttribute("tabindex")).toBe("-1");
});

it("marks an unavailable switch's input aria-disabled and its opening focus, following the props", () => {
  const { container, render } = mount(<Switch label="On" selected={false} onSelectedChange={() => {}} unavailable opensFocused />);
  const input = container.querySelector("input")!;
  expect(input.getAttribute("aria-disabled")).toBe("true");
  expect(input.hasAttribute("data-opening-focus")).toBe(true);
  render(<Switch label="On" selected={false} onSelectedChange={() => {}} />);
  expect(input.hasAttribute("aria-disabled")).toBe(false);
  expect(input.hasAttribute("data-opening-focus")).toBe(false);
});

it("describes a checkbox by its description before the further ids it is given", () => {
  const { container } = mount(
    <>
      <Checkbox selected onSelectedChange={() => {}} describedBy="warning" description="Note">Option</Checkbox>
      <p id="warning">Warning</p>
    </>,
  );
  const ids = container.querySelector("input")!.getAttribute("aria-describedby")!.split(" ");
  expect(document.getElementById(ids[0]!)?.textContent).toBe("Note");
  expect(ids).toContain("warning");
});

it("announces a disabled list box item as unavailable and passes an activated item's id", () => {
  const onAction = vi.fn();
  const { container } = mount(
    <ListBox label="Entries" onAction={onAction}>
      <ListBoxItem id="a" textValue="A">A</ListBoxItem>
      <ListBoxItem id="b" textValue="B" disabled>B</ListBoxItem>
    </ListBox>,
  );
  const options = container.querySelectorAll<HTMLElement>('[role="option"]');
  expect(options[1]!.getAttribute("aria-disabled")).toBe("true");
  pressWithMouse(options[0]!);
  expect(onAction).toHaveBeenCalledWith("a");
});
