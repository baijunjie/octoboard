// @vitest-environment jsdom
import { expect, it } from "vitest";

import { disabledGroupProps } from "./Field";
import { hasKeyboardFocus, useFocusVisibleProps } from "./focus";
import { mount } from "./testSupport";

it("marks a group disabled only while it is", () => {
  expect(disabledGroupProps(true)).toEqual({ "data-disabled": true });
  expect(disabledGroupProps(false)).toEqual({});
  expect(disabledGroupProps(undefined)).toEqual({});
});

it("reports keyboard focus from the props of a hand-built focusable element", () => {
  let seen: boolean | undefined;
  function Probe() {
    const props = useFocusVisibleProps();
    seen = hasKeyboardFocus(props);
    return <div tabIndex={0} {...props} />;
  }
  mount(<Probe />);
  expect(seen).toBe(false);
  expect(hasKeyboardFocus({ "data-focus-visible": true })).toBe(true);
});
