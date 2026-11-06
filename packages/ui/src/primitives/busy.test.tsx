// @vitest-environment jsdom
import { expect, it } from "vitest";

import { BareButton, Button } from "./Button";
import { Checkbox } from "./Checkbox";
import { ColorSwatch, ColorSwatchPicker } from "./ColorSwatchPicker";
import { ComboBox, ComboBoxInputGroup, ComboBoxPopover, ComboBoxTrigger } from "./ComboBox";
import { Disclosure, DisclosureContent, DisclosureHeading, DisclosureTrigger } from "./Disclosure";
import { Label } from "./Field";
import { ListBox, ListBoxItem } from "./ListBox";
import { Menu, MenuList, MenuPopover, MenuTrigger } from "./Menu";
import { Modal, ModalBody, ModalCloseButton, ModalHeading } from "./Modal";
import { Radio, RadioGroup } from "./Radio";
import { Select, SelectPopover, SelectTrigger } from "./Select";
import { Switch } from "./Switch";
import { Tab, TabList, TabListContainer, Tabs } from "./Tabs";
import { Tag, TagGroup, TagList, TagRemoveButton } from "./Tag";
import { mount } from "./testSupport";
import { Input, SearchField, TextArea, TextField } from "./TextField";
import { ToggleButton, ToggleButtonGroup } from "./ToggleButton";
import { Tree, TreeExpandButton, TreeItem } from "./Tree";
import { useBusy } from "./busy";

function Everything({ busy }: { busy: boolean }): React.ReactElement {
  return (
    <Modal busy={busy} onClose={() => {}}>
      <ModalHeading>Form</ModalHeading>
      <ModalCloseButton label="Close" />
      <ModalBody>
        <TextField label="Name" value="" onChange={() => {}}>
          <Input />
        </TextField>
        <TextField label="Notes" value="" onChange={() => {}}>
          <TextArea />
        </TextField>
        <Input label="Loose" value="" onChange={() => {}} />
        <Input label="Adorned" value="" onChange={() => {}} start={<span>@</span>} />
        <SearchField label="Filter" value="" onChange={() => {}} />
        <Select label="Agent" value={null} onChange={() => {}}>
          <Label>Agent</Label>
          <SelectTrigger value="Pick" />
          <SelectPopover>{null}</SelectPopover>
        </Select>
        <Switch label="Share" selected={false} onSelectedChange={() => {}} />
        <Checkbox selected={false} onSelectedChange={() => {}}>Include</Checkbox>
        <RadioGroup label="Scope" value="a" onChange={() => {}}>
          <Radio value="a">A</Radio>
          <Radio value="b">B</Radio>
        </RadioGroup>
        <ToggleButton label="Match case" selected={false} onSelectedChange={() => {}}>Aa</ToggleButton>
        <Tabs defaultSelectedKey="one">
          <TabListContainer>
            <TabList label="Sections">
              <Tab id="one">One</Tab>
              <Tab id="two">Two</Tab>
            </TabList>
          </TabListContainer>
        </Tabs>
        <Disclosure>
          <DisclosureHeading>
            <DisclosureTrigger>Advanced</DisclosureTrigger>
          </DisclosureHeading>
          <DisclosureContent>hidden</DisclosureContent>
        </Disclosure>
        <Menu open={false} onOpenChange={() => {}}>
          <MenuTrigger label="More">…</MenuTrigger>
          <MenuPopover><MenuList onAction={() => {}}>{null}</MenuList></MenuPopover>
        </Menu>
        <ComboBox inputValue="" onInputChange={() => {}} onPick={() => {}} items={[{ id: "one" }]}>
          <Label>Suggest</Label>
          <ComboBoxInputGroup>
            <Input aria-label="Suggest" />
            <ComboBoxTrigger label="Suggestions" />
          </ComboBoxInputGroup>
          <ComboBoxPopover>{null}</ComboBoxPopover>
        </ComboBox>
        <ListBox label="Choices">
          <ListBoxItem id="x" textValue="X">X</ListBoxItem>
          <ListBoxItem id="y" textValue="Y">Y</ListBoxItem>
        </ListBox>
        <Tree label="Files" items={[{ id: "dir" }]} expandedKeys={new Set<string>()} onExpandedChange={() => {}} onAction={() => {}} rowHeight={32}>
          {(item) => (
            <TreeItem id={item.id} textValue={item.id} content={<TreeExpandButton label="Expand">&gt;</TreeExpandButton>} />
          )}
        </Tree>
        <TagGroup label="Tags" onRemove={() => {}}>
          <TagList>
            <Tag id="alpha">alpha<TagRemoveButton label="Remove alpha" /></Tag>
          </TagList>
        </TagGroup>
        <ColorSwatchPicker labelledBy="swatches" value="#ff0000" onChange={() => {}}>
          <ColorSwatch color="#ff0000" />
          <ColorSwatch color="#00ff00" />
        </ColorSwatchPicker>
        <ToggleButtonGroup label="Mode" defaultSelectedKey="a">
          <ToggleButton id="a">A</ToggleButton>
          <ToggleButton id="b">B</ToggleButton>
        </ToggleButtonGroup>
        <BareButton>Bare</BareButton>
        <Button>Cancel</Button>
        <Button pending>Save</Button>
      </ModalBody>
    </Modal>
  );
}

/** Every element of the page that takes input. React Aria's visually hidden dismiss button and HeroUI's scroll buttons
 * of a tab strip are not controls of the form: they are out of the tab order and scroll or dismiss only. */
function controls(): HTMLElement[] {
  return [...document.querySelectorAll<HTMLElement>("input, textarea, button, [role=tab], [role=switch], [role=option], [role=row], [role=radio]")]
    .filter((element) => element.getAttribute("type") !== "hidden" && !/^(Dismiss|Scroll tabs)/.test(element.getAttribute("aria-label") ?? ""));
}

const isDisabled = (element: HTMLElement) => element.matches(":disabled, [aria-disabled=true], [data-disabled=true]");

it("disables every control of a busy modal but the pending button", () => {
  mount(<Everything busy />);
  const pending = document.querySelector<HTMLButtonElement>("[data-pending=true]")!;
  // A pending button is the one control left: focusable, and `aria-disabled` only as it takes no press.
  expect(pending.disabled).toBe(false);
  expect(pending.getAttribute("tabindex")).toBe("0");
  const others = controls().filter((element) => element !== pending);
  expect(others.length).toBeGreaterThan(15);
  expect(others.filter((element) => !isDisabled(element)).map((element) => element.outerHTML.slice(0, 80))).toEqual([]);
});

it("leaves the controls of a modal that is not busy operable", () => {
  mount(<Everything busy={false} />);
  const operable = controls().filter((element) => !isDisabled(element));
  expect(operable.length).toBeGreaterThan(10);
  expect(operable.map((element) => element.textContent)).toContain("Cancel");
  expect(document.querySelector<HTMLButtonElement>('button[aria-label="Close"]')!.disabled).toBe(false);
});

it("enables the controls again when the modal stops being busy", () => {
  const { render } = mount(<Everything busy />);
  expect(document.querySelector<HTMLInputElement>('input[aria-label="Name"]')!.disabled).toBe(true);
  render(<Everything busy={false} />);
  expect(document.querySelector<HTMLInputElement>('input[aria-label="Name"]')!.disabled).toBe(false);
});

it("is not busy outside a modal, and a modal opened from a busy one is its own", () => {
  function Probe({ name }: { name: string }): React.ReactElement {
    return <span data-probe={name}>{String(useBusy())}</span>;
  }
  mount(
    <>
      <Probe name="outside" />
      <Modal busy onClose={() => {}}>
        <Probe name="busy" />
        <Modal onClose={() => {}}>
          <Probe name="nested" />
        </Modal>
      </Modal>
    </>,
  );
  const read = (name: string) => document.querySelector(`[data-probe=${name}]`)!.textContent;
  expect([read("outside"), read("busy"), read("nested")]).toEqual(["false", "true", "false"]);
});

it("disables the collections' items, the combo box and the toggles of a busy modal", () => {
  mount(<Everything busy />);
  const expectDisabled = (selector: string, count: number) => {
    const found = [...document.querySelectorAll<HTMLElement>(selector)];
    expect(found, selector).toHaveLength(count);
    expect(found.filter((element) => !isDisabled(element)).map((element) => element.outerHTML.slice(0, 80)), selector).toEqual([]);
  };
  expectDisabled('input[aria-label="Suggest"], button[data-slot="combo-box-trigger"]', 2);
  expectDisabled('[data-slot="list-box-item"]', 2);
  expectDisabled('[role="treegrid"] [role="row"], [role="treegrid"] button', 2);
  expectDisabled('[data-slot="tag"], [data-slot="tag-remove-button"]', 2);
  expectDisabled('[data-slot="color-swatch-picker-item"]', 2);
  // The lone toggle and the group's two options.
  expectDisabled('[data-slot="toggle-button"]', 3);
});
