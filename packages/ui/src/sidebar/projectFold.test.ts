import { describe, expect, it } from "vitest";

import { pinAfterFoldAction, projectFoldControl, reconcileExpandPin, reconcileExpandPins } from "./projectFold";

const ids = ["a", "b", "c"];
const collapsed = (...open: string[]) => new Set(ids.filter((id) => !open.includes(id)));

describe("projectFoldControl", () => {
  it.each([
    { name: "collapse while any project is expanded", pin: false, open: ["a", "b", "c"], expected: "collapse" },
    { name: "collapse when only some are collapsed", pin: false, open: ["b", "c"], expected: "collapse" },
    { name: "expand when every project is collapsed", pin: false, open: [], expected: "expand" },
    { name: "stay on expand while some stay collapsed", pin: true, open: ["a"], expected: "expand" },
    { name: "return to collapse once every project is expanded", pin: true, open: ["a", "b", "c"], expected: "collapse" },
    { name: "stay on expand while every project is collapsed", pin: true, open: [], expected: "expand" },
  ] as const)("$name", ({ pin, open, expected }) => {
    expect(projectFoldControl(pin, ids, collapsed(...open))).toBe(expected);
  });
});

describe("pinAfterFoldAction", () => {
  it.each([
    { control: "collapse" as const, pin: true },
    { control: "expand" as const, pin: false },
  ])("pressing $control sets the pin to $pin", ({ control, pin }) => {
    expect(pinAfterFoldAction(control)).toBe(pin);
  });
});

describe("reconcileExpandPin", () => {
  it.each([
    { name: "clears once every project is expanded", pin: true, open: ["a", "b", "c"], expected: false },
    { name: "sets once every project is collapsed", pin: false, open: [], expected: true },
    { name: "keeps a clear pin while the list is mixed", pin: false, open: ["a"], expected: false },
    { name: "keeps a set pin while the list is mixed", pin: true, open: ["a"], expected: true },
    { name: "keeps the pin when there is no project", pin: true, open: [] as string[], expected: true, projectIds: [] as string[] },
  ])("$name", ({ pin, open, expected, projectIds }) => {
    expect(reconcileExpandPin(pin, projectIds ?? ids, collapsed(...open))).toBe(expected);
  });
});

describe("the fold button over a console", () => {
  const step = (pin: boolean, open: string[]) => {
    const next = reconcileExpandPin(pin, ids, collapsed(...open));
    return { pin: next, control: projectFoldControl(next, ids, collapsed(...open)) };
  };

  it("stays on expand after a collapse until every project is opened by hand", () => {
    expect(projectFoldControl(false, ids, collapsed("a", "b", "c"))).toBe("collapse");

    let pin = pinAfterFoldAction("collapse");
    let state = step(pin, []);
    expect(state).toEqual({ pin: true, control: "expand" });

    state = step(state.pin, ["a"]);
    expect(state).toEqual({ pin: true, control: "expand" });

    state = step(state.pin, ["a", "b", "c"]);
    expect(state).toEqual({ pin: false, control: "collapse" });

    state = step(state.pin, ["a", "b"]);
    expect(state).toEqual({ pin: false, control: "collapse" });

    state = step(state.pin, []);
    expect(state).toEqual({ pin: true, control: "expand" });
  });

  it("flips on a press that a filter kept from reaching every project, then waits for the rest", () => {
    let pin = pinAfterFoldAction("collapse");
    // The press collapsed a and b. c stayed expanded because the filter hid it.
    let state = step(pin, ["c"]);
    expect(state).toEqual({ pin: true, control: "expand" });

    state = step(state.pin, ["a", "c"]);
    expect(state).toEqual({ pin: true, control: "expand" });

    state = step(state.pin, ["a", "b", "c"]);
    expect(state).toEqual({ pin: false, control: "collapse" });
  });
});

describe("reconcileExpandPins", () => {
  const projects = [
    { id: "a", console_id: "one" },
    { id: "b", console_id: "one" },
    { id: "c", console_id: "two" },
  ];

  it("moves only the console whose projects are all one way", () => {
    const next = reconcileExpandPins(new Map([["two", true]]), projects, new Set(["a", "b"]));
    expect(next).toEqual(
      new Map([
        ["two", false],
        ["one", true],
      ]),
    );
    expect(reconcileExpandPins(next!, projects, new Set(["a", "b"]))).toBeUndefined();
  });
});
