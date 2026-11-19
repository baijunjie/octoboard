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
  // A flex row, not the labelled grid.
  for (const name of ["grid", "grid-cols-[auto_minmax(0,1fr)]", "grid-flow-col"]) expect(row.classList).not.toContain(name);
});

it("lays a labelled row out as a grid of a label column, a trigger column taking the rest and one auto column per later child, at the field row gap", () => {
  const { container } = mount(
    <FieldRow labelled className="flex-1">
      <div className="col-span-2 grid grid-cols-subgrid">
        <span>Branch</span>
        <button type="button">main</button>
      </div>
      <button type="button">Copy</button>
    </FieldRow>,
  );
  const row = container.firstElementChild as HTMLElement;
  expect(Array.from(row.children).map((child) => child.tagName)).toEqual(["DIV", "BUTTON"]);
  for (const name of ["grid", "grid-cols-[auto_minmax(0,1fr)]", "grid-flow-col", "items-center", "min-w-0", "gap-(--field-row-gap)", "flex-1"]) expect(row.classList).toContain(name);
  expect(row.classList).not.toContain("flex");
});
