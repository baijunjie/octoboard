// @vitest-environment jsdom
import { expect, it } from "vitest";

import type { CellValue } from "../../protocol";
import { mountHost } from "../../test/mountHost";
import { useCellText } from "./cells";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function Texts({ values }: { values: CellValue[] }) {
  const cellText = useCellText();
  return <>{values.map((value, index) => <span key={index}>{cellText(value)}</span>)}</>;
}

const textsOf = (values: CellValue[]) => [...mountHost(<Texts values={values} />).querySelectorAll("span")].map((span) => span.textContent);

it("shows a text as it is, a number grouped in the UI language and a boolean as Yes or No", () => {
  expect(textsOf(["plain <b>text</b>", "", 1234567, 0, -1.5, 0.25, true, false])).toEqual(["plain <b>text</b>", "", "1,234,567", "0", "-1.5", "0.25", "Yes", "No"]);
});

it("keeps a long fraction rather than rounding it to a few places", () => {
  expect(textsOf([0.123456789])).toEqual(["0.123456789"]);
});
