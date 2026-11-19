// @vitest-environment jsdom
import { act, useState } from "react";
import { expect, it, vi } from "vitest";

import { mountHost } from "../../test/mountHost";
import { DEFAULT_FIND_OPTIONS, type FindOptions } from "./find";
import { FindOptionToggles } from "./FindBar";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const LABELS = ["Match case", "Whole word", "Use regular expression"] as const;
const toggle = (label: string) => document.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;

it("draws one toggle each for Match case, Whole word and Use regular expression, pressed as the options say", () => {
  mountHost(<FindOptionToggles options={{ matchCase: true, wholeWord: false, regex: true }} onOptionsChange={() => {}} />);
  expect(Array.from(document.querySelectorAll("button"), (button) => button.getAttribute("aria-label"))).toEqual([...LABELS]);
  expect(LABELS.map((label) => toggle(label).getAttribute("aria-pressed"))).toEqual(["true", "false", "true"]);
});

it.each([
  ["Match case", { matchCase: true, wholeWord: false, regex: false }],
  ["Whole word", { matchCase: false, wholeWord: true, regex: false }],
  ["Use regular expression", { matchCase: false, wholeWord: false, regex: true }],
] as const)("reports the options with only %s turned on when it is pressed", (label, expected) => {
  const onChange = vi.fn();
  mountHost(<FindOptionToggles options={DEFAULT_FIND_OPTIONS} onOptionsChange={onChange} />);
  act(() => toggle(label).click());
  expect(onChange).toHaveBeenCalledExactlyOnceWith(expected);
});

it("keeps the other fields of a larger set of options as it changes its own", () => {
  interface Search extends FindOptions {
    fileMask: string;
  }
  const seen: Search[] = [];
  function Host() {
    const [options, setOptions] = useState<Search>({ ...DEFAULT_FIND_OPTIONS, fileMask: "*.ts" });
    seen.push(options);
    return <FindOptionToggles options={options} onOptionsChange={setOptions} />;
  }
  mountHost(<Host />);
  act(() => toggle("Whole word").click());
  expect(seen.at(-1)).toEqual({ matchCase: false, wholeWord: true, regex: false, fileMask: "*.ts" });
  act(() => toggle("Whole word").click());
  expect(seen.at(-1)).toEqual({ matchCase: false, wholeWord: false, regex: false, fileMask: "*.ts" });
});
