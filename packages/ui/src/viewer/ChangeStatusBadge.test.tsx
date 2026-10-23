import { renderToStaticMarkup } from "react-dom/server";
import { expect, it } from "vitest";

import { ChangeStatusBadge } from "./ChangeStatusBadge";

// The change list's letter and the viewer's word are both this badge, so one test covers the colour of both.
it("marks every status with its colour, untracked as the neutral one, always soft", () => {
  const colours = { added: "success", deleted: "danger", conflicted: "danger", modified: "warning", renamed: "accent", typeChanged: "accent", untracked: "default" } as const;
  for (const [status, colour] of Object.entries(colours)) {
    const html = renderToStaticMarkup(<ChangeStatusBadge status={status as keyof typeof colours}>x</ChangeStatusBadge>);
    expect(html).toContain(`data-color="${colour}"`);
    expect(html).toContain('data-variant="soft"');
  }
});

it("leaves out the comma, since neither the letter nor the word has a name right before it", () => {
  expect(renderToStaticMarkup(<ChangeStatusBadge status="added">x</ChangeStatusBadge>)).not.toContain("sr-only");
  expect(renderToStaticMarkup(<ChangeStatusBadge status="added" separated>x</ChangeStatusBadge>)).toContain("sr-only");
});
