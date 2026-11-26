// @vitest-environment jsdom
import React, { act } from "react";
import { expect, it, vi } from "vitest";

import { Button } from "./Button";
import { Checkbox } from "./Checkbox";
import { Label, LOCKED_FIELD_CLASS } from "./Field";
import { ListBox, ListBoxItem } from "./ListBox";
import { BusyContext } from "./busy";
import { NumberField } from "./NumberField";
import { Radio, RadioGroup } from "./Radio";
import { Slider } from "./Slider";
import { Switch } from "./Switch";
import { mount, pressWithMouse } from "./testSupport";
import { Input, SearchField, TextArea, TextField } from "./TextField";

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

it("makes a search field's input read-only while it stays focusable, and editable again without the prop", () => {
  const field = (readOnly: boolean) => <SearchField label="Filter" value="abc" onChange={() => {}} readOnly={readOnly} />;
  const { container, render } = mount(field(true));
  const input = container.querySelector("input")!;
  expect(input.readOnly).toBe(true);
  expect(input.value).toBe("abc");
  act(() => input.focus());
  expect(document.activeElement).toBe(input);
  render(field(false));
  expect(input.readOnly).toBe(false);
});

it("marks an unavailable switch's input aria-disabled and its opening focus, following the props", () => {
  const { container, render } = mount(<Switch label="On" selected={false} onSelectedChange={() => {}} unavailable opensFocused />);
  const input = container.querySelector("input")!;
  expect(input.getAttribute("aria-disabled")).toBe("true");
  expect(input.hasAttribute("data-opening-focus")).toBe(true);
  render(<Switch label="On" selected={false} onSelectedChange={() => {}} unavailable={undefined} />);
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

const lockedClasses = LOCKED_FIELD_CLASS.split(" ");
const hasLockedLook = (element: Element) => lockedClasses.every((name) => element.classList.contains(name));

it("draws an input and a textarea of a locked field as a value to read, and those of an unlocked one as controls", () => {
  const field = (locked: boolean) => (
    <>
      <TextField value="abc" onChange={() => {}} readOnly locked={locked}>
        <Label>Name</Label>
        <Input />
      </TextField>
      <TextField value="abc" onChange={() => {}} readOnly locked={locked}>
        <Label>Story</Label>
        <TextArea />
      </TextField>
    </>
  );
  const { container, render } = mount(field(true));
  const controls = [container.querySelector("input")!, container.querySelector("textarea")!];
  for (const control of controls) {
    expect(hasLockedLook(control)).toBe(true);
    expect(control.readOnly).toBe(true);
    expect(control.value).toBe("abc");
  }
  render(field(false));
  for (const control of controls) expect(lockedClasses.some((name) => control.classList.contains(name))).toBe(false);
});

it("gives a textarea's element to its ref, and keeps its size fixed when it is not to be resized", () => {
  const ref = React.createRef<HTMLTextAreaElement>();
  const { container } = mount(
    <>
      <TextField value="" onChange={() => {}}>
        <Label>Story</Label>
        <TextArea ref={ref} resizable={false} />
      </TextField>
      <TextField value="" onChange={() => {}}>
        <Label>Notes</Label>
        <TextArea />
      </TextField>
    </>,
  );
  const [fixed, resizable] = [...container.querySelectorAll("textarea")];
  expect(ref.current).toBe(fixed);
  expect(fixed.classList.contains("resize-none")).toBe(true);
  expect(resizable.classList.contains("resize-none")).toBe(false);
});

it("says a textarea offers suggestions, naming its list and the one the keys are on only while the list shows, and stays a text box", () => {
  const field = (suggestions?: { list: string; expanded: boolean; active?: string }) => (
    <TextField value="" onChange={() => {}}>
      <Label>Message</Label>
      <TextArea suggestions={suggestions} />
    </TextField>
  );
  const NAMES = ["aria-autocomplete", "aria-controls", "aria-activedescendant"];
  const { container, render } = mount(field());
  const area = () => container.querySelector("textarea")!;
  for (const name of NAMES) expect(area().hasAttribute(name)).toBe(false);
  render(field({ list: "list-1", expanded: false, active: "list-1-option-2" }));
  expect(area().getAttribute("aria-autocomplete")).toBe("list");
  expect(area().hasAttribute("aria-controls")).toBe(false);
  expect(area().hasAttribute("aria-activedescendant")).toBe(false);
  render(field({ list: "list-1", expanded: true, active: "list-1-option-2" }));
  expect(area().getAttribute("aria-controls")).toBe("list-1");
  expect(area().getAttribute("aria-activedescendant")).toBe("list-1-option-2");
  // ARIA in HTML allows a textarea no role: it is never made a combo box.
  expect(area().hasAttribute("role")).toBe(false);
  expect(area().hasAttribute("aria-expanded")).toBe(false);
  render(field({ list: "list-1", expanded: true }));
  expect(area().hasAttribute("aria-activedescendant")).toBe(false);
  render(field());
  for (const name of NAMES) expect(area().hasAttribute(name)).toBe(false);
});

it("leaves a read-only field that is not locked drawn as a control", () => {
  const { container } = mount(
    <TextField value="abc" onChange={() => {}} readOnly>
      <Label>Name</Label>
      <Input />
    </TextField>,
  );
  expect(container.querySelector("input")!.classList.contains("bg-transparent")).toBe(false);
});

it("says an input, a textarea and a radio group need a value for assistive technology only, drawing no mark", () => {
  const { container } = mount(
    <>
      <TextField value="" onChange={() => {}}>
        <Label>Name</Label>
        <Input required />
      </TextField>
      <TextField value="" onChange={() => {}}>
        <Label>Story</Label>
        <TextArea required />
      </TextField>
      <RadioGroup label="Size" value="" onChange={() => {}} required>
        <Radio value="s">Small</Radio>
      </RadioGroup>
      <RadioGroup label="Colour" value="" onChange={() => {}}>
        <Radio value="r">Red</Radio>
      </RadioGroup>
    </>,
  );
  expect(container.querySelector("input")!.getAttribute("aria-required")).toBe("true");
  expect(container.querySelector("textarea")!.getAttribute("aria-required")).toBe("true");
  const [required, optional] = [...container.querySelectorAll('[role="radiogroup"]')];
  expect(required.getAttribute("aria-required")).toBe("true");
  expect(optional.hasAttribute("aria-required")).toBe(false);
  // HeroUI draws its asterisk after the label of a field marked `data-required`: the text fields are not marked, and the
  // group, which is, takes the asterisk off its labels.
  expect(container.querySelector('[data-slot="textfield"][data-required="true"]')).toBeNull();
  expect(required.className).toContain("after:content-none");
  expect(optional.className).not.toContain("after:content-none");
});

it("draws a locked slider at full strength where a disabled one is dimmed, with its value still read", () => {
  const slider = (disabled: boolean, locked: boolean) => <Slider label="Level" value={4} onChange={() => {}} min={0} max={10} disabled={disabled} locked={locked} />;
  const { container, render } = mount(slider(true, false));
  const root = () => container.querySelector<HTMLElement>('[data-slot="slider"]')!;
  expect(root().className).not.toContain("opacity-100");
  render(slider(true, true));
  expect(root().className).toContain("data-[disabled=true]:opacity-100");
  expect(container.querySelector("input[type=range]")!.hasAttribute("disabled")).toBe(true);
  expect((container.querySelector("input[type=range]") as HTMLInputElement).value).toBe("4");
});

function NumberFieldHarness({ initial = 5, onChange }: { initial?: number; onChange?: (value: number) => void }): React.ReactElement {
  const [value, setValue] = React.useState(initial);
  return <NumberField label="Size" value={value} onChange={(next) => { setValue(next); onChange?.(next); }} min={0} max={10} step={1} incrementLabel="Bigger" decrementLabel="Smaller" />;
}

it("names a number field and shows only its value, with no unit or description", () => {
  const { container } = mount(<NumberFieldHarness />);
  const input = container.querySelector("input")!;
  expect(input.getAttribute("aria-label")).toBe("Size");
  expect(input.value).toBe("5");
  expect(input.hasAttribute("aria-describedby")).toBe(false);
  expect(container.textContent).toBe("");
  expect(container.querySelector('[data-slot="number-field-group"]')!.hasAttribute("data-suffixed")).toBe(false);
});

it("draws a number field's label above it and a description under it where asked, naming the input by them", () => {
  const { container } = mount(
    <NumberField label="Day of month" visibleLabel description="A short month runs on its last day." value={5} onChange={() => {}} min={1} max={31} incrementLabel="Bigger" decrementLabel="Smaller" />,
  );
  const label = container.querySelector("[data-slot=label]")!;
  const group = container.querySelector("[data-slot=number-field-group]")!;
  const description = container.querySelector("[data-slot=description]")!;
  const input = container.querySelector("input")!;
  expect(label.textContent).toBe("Day of month");
  expect(label.compareDocumentPosition(group) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(group.compareDocumentPosition(description) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(input.hasAttribute("aria-label")).toBe(false);
  expect(input.getAttribute("aria-labelledby")).toContain(label.id);
  expect(description.textContent).toBe("A short month runs on its last day.");
  expect(input.getAttribute("aria-describedby")).toContain(description.id);
});

it("links a number field to its description only while one is drawn", () => {
  const field = (description?: string) => (
    <NumberField label="Day of month" visibleLabel description={description} value={5} onChange={() => {}} min={1} max={31} incrementLabel="Bigger" decrementLabel="Smaller" />
  );
  const { container, render } = mount(field("A short month runs on its last day."));
  const description = container.querySelector("[data-slot=description]")!;
  expect(container.querySelector("input")!.getAttribute("aria-describedby")).toContain(description.id);
  render(field());
  expect(container.querySelector("[data-slot=description]")).toBeNull();
  // No attribute is left naming an element that is gone.
  const withoutNote = container.querySelector("input")!;
  for (const id of (withoutNote.getAttribute("aria-describedby") ?? "").split(" ").filter(Boolean)) expect(document.getElementById(id), id).not.toBeNull();
  render(field("Back again."));
  const again = container.querySelector("[data-slot=description]")!;
  expect(again.textContent).toBe("Back again.");
  expect(container.querySelector("input")!.getAttribute("aria-describedby")).toContain(again.id);
});

it("steps a number field by its buttons and arrow keys, and disables a button at its bound", () => {
  const onChange = vi.fn();
  const { container } = mount(<NumberFieldHarness initial={9} onChange={onChange} />);
  const input = container.querySelector("input")!;
  const increment = container.querySelector<HTMLButtonElement>('[slot="increment"]')!;
  const decrement = container.querySelector<HTMLButtonElement>('[slot="decrement"]')!;
  act(() => increment.click());
  expect(onChange).toHaveBeenLastCalledWith(10);
  expect(increment.disabled).toBe(true);
  act(() => decrement.click());
  expect(onChange).toHaveBeenLastCalledWith(9);
  act(() => void input.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true })));
  expect(onChange).toHaveBeenLastCalledWith(8);
  act(() => void input.dispatchEvent(new KeyboardEvent("keydown", { key: "Home", bubbles: true })));
  expect(onChange).toHaveBeenLastCalledWith(0);
  expect(decrement.disabled).toBe(true);
});

it("commits a typed number on blur, clamped into the range, and never reports an emptied input", () => {
  const onChange = vi.fn();
  const { container } = mount(<NumberFieldHarness onChange={onChange} />);
  const input = container.querySelector("input")!;
  const type = (text: string) => {
    act(() => {
      input.focus();
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, text);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
    act(() => input.blur());
  };
  type("7");
  expect(onChange).toHaveBeenLastCalledWith(7);
  type("40");
  expect(onChange).toHaveBeenLastCalledWith(10);
  expect(input.value).toBe("10");
  onChange.mockClear();
  type("");
  expect(onChange).not.toHaveBeenCalled();
});

it("names the stepper buttons by the labels it is given and leaves them out of the tab order", () => {
  const { container } = mount(<NumberFieldHarness />);
  const increment = container.querySelector<HTMLButtonElement>('[slot="increment"]')!;
  const decrement = container.querySelector<HTMLButtonElement>('[slot="decrement"]')!;
  expect(increment.getAttribute("aria-label")).toBe("Bigger");
  expect(decrement.getAttribute("aria-label")).toBe("Smaller");
  expect(increment.tabIndex).toBe(-1);
  expect(decrement.tabIndex).toBe(-1);
});

it("hands the input element to the ref it is given, so a caller can move focus to it", () => {
  const inputRef = React.createRef<HTMLInputElement>();
  const { container } = mount(<NumberField label="Size" value={3} onChange={() => {}} min={0} max={10} incrementLabel="Bigger" decrementLabel="Smaller" inputRef={inputRef} />);
  const input = container.querySelector("input")!;
  expect(inputRef.current).toBe(input);
  act(() => inputRef.current!.focus());
  expect(document.activeElement).toBe(input);
});

it("disables a number field's input and buttons in a busy modal", () => {
  const { container } = mount(
    <BusyContext.Provider value>
      <NumberField label="Size" value={3} onChange={() => {}} min={0} max={10} incrementLabel="Bigger" decrementLabel="Smaller" />
    </BusyContext.Provider>,
  );
  expect(container.querySelector("input")!.disabled).toBe(true);
  expect([...container.querySelectorAll("button")].every((button) => button.disabled)).toBe(true);
});
