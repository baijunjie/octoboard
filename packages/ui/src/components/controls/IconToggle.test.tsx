// @vitest-environment jsdom
import { expect, it } from "vitest";

import { FIELD_CONTROL_CLASS } from "../../primitives/Button";
import { mount } from "../../primitives/testSupport";
import { IconToggle } from "./IconToggle";

function toggle(besideField: boolean): DOMTokenList {
  const { container } = mount(
    <IconToggle label="Include ignored files" isSelected={false} onChange={() => {}} besideField={besideField}>
      <svg aria-hidden="true" />
    </IconToggle>,
  );
  return container.querySelector("button")!.classList;
}

it("stands beside a field in the field's shape and the filled look rather than as a row's icon", () => {
  const classes = toggle(true);
  expect(classes).toContain(FIELD_CONTROL_CLASS);
  expect(classes).not.toContain("toggle-button--ghost");
  expect(classes).toContain("icon-toggle");
  expect(classes).not.toContain("row-icon-control");
});

it("is a row's icon when it does not stand beside a field", () => {
  const classes = toggle(false);
  expect(classes).toContain("row-icon-control");
  expect(classes).not.toContain(FIELD_CONTROL_CLASS);
});
