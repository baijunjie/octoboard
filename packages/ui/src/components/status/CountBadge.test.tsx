import { renderToStaticMarkup } from "react-dom/server";
import { expect, it } from "vitest";

import { CountBadge } from "./CountBadge";

const html = (element: React.ReactElement) => renderToStaticMarkup(element);

it("draws a count of one digit as a circle and a longer count as a pill", () => {
  const circle = html(<CountBadge count={4} />);
  expect(circle).toContain("inline-block w-5 px-0 text-center");

  for (const count of [12, 100]) {
    const pill = html(<CountBadge count={count} />);
    expect(pill).toContain("px-2");
    expect(pill).not.toContain("px-0");
    expect(pill).not.toContain("inline-block");
  }
});

it("keeps the count out of the accessibility tree and takes the success colour", () => {
  const markup = html(<CountBadge count={3} color="success" />);
  expect(markup).toContain('aria-hidden="true"');
  expect(markup).toContain('data-color="success"');
  expect(markup).not.toContain("sr-only");
});
