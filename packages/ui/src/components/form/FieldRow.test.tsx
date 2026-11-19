// @vitest-environment jsdom
import { expect, it } from "vitest";

import { mount } from "../../primitives/testSupport";
import { FieldRow } from "./FieldRow";

it("lays a field and what stands beside it out on one centred line, spaced by the field row gap", () => {
  const { container } = mount(
    <FieldRow className="w-80">
      <input aria-label="Path" />
      <button type="button">Browse</button>
    </FieldRow>,
  );
  const row = container.firstElementChild as HTMLElement;
  expect(Array.from(row.children).map((child) => child.tagName)).toEqual(["INPUT", "BUTTON"]);
  for (const name of ["flex", "items-center", "min-w-0", "gap-(--field-row-gap)", "w-80"]) expect(row.classList).toContain(name);
});
