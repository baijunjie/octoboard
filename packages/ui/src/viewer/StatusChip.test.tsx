import { renderToStaticMarkup } from "react-dom/server";
import { expect, it } from "vitest";

import { StatusChip } from "./StatusChip";

// The change list's letter and the viewer's word are both this chip, so one test covers the colour of both.
it("gives an untracked chip its own colour class and every other status HeroUI's", () => {
  expect(renderToStaticMarkup(<StatusChip status="untracked">U</StatusChip>)).toContain("chip-untracked");
  for (const status of ["added", "deleted", "modified", "renamed", "typeChanged", "conflicted"] as const) {
    expect(renderToStaticMarkup(<StatusChip status={status}>x</StatusChip>)).not.toContain("chip-untracked");
  }
});
