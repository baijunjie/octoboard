import { expect, it } from "vitest";

import { type DockEvent, initialDockMotion, reduceDockMotion } from "./sidebarDockMotion";

const open = initialDockMotion(true, false);
const closed = initialDockMotion(false, false);

function run(from: ReturnType<typeof initialDockMotion>, ...events: DockEvent[]) {
  return events.reduce(reduceDockMotion, from);
}

it("eases the column closed, and only then allows the hover card", () => {
  const closing = run(open, { type: "target", shown: false, peekActive: false, reduce: false });
  expect(closing.phase).toBe("closing");
  expect(closing.inset).toBe(false);

  const stillColumn = run(closing, { type: "target", shown: false, peekActive: true, reduce: false });
  expect(stillColumn.phase).toBe("closing");
  expect(stillColumn.inset).toBe(false);

  const settled = run(stillColumn, { type: "column-end", reduce: false });
  expect(settled.phase).toBe("closed");
  expect(settled.inset).toBe(true);
  expect(settled.insetShown).toBe(false);
  expect(settled.pending).toBe("show-inset");

  const onScreen = run(settled, { type: "painted" });
  expect(onScreen.insetShown).toBe(true);
  expect(onScreen.pending).toBe(null);
});

it("hides and shows with no ease where motion is reduced", () => {
  const hidden = run(open, { type: "target", shown: false, peekActive: false, reduce: true });
  expect(hidden.phase).toBe("closed");
  expect(hidden.inset).toBe(false);

  const shown = run(hidden, { type: "target", shown: true, peekActive: false, reduce: true });
  expect(shown.phase).toBe("open");
  expect(shown.inset).toBe(false);
});

it("eases a hidden column open without passing through the hover card", () => {
  const opening = run(closed, { type: "target", shown: true, peekActive: false, reduce: false });
  expect(opening.phase).toBe("opening");
  expect(opening.inset).toBe(false);
  expect(run(opening, { type: "column-end", reduce: false }).phase).toBe("open");
});

it("paints the column at no width before easing it open off the hover card", () => {
  const card = run(closed, { type: "target", shown: false, peekActive: true, reduce: false }, { type: "painted" });
  expect(card.insetShown).toBe(true);

  const held = run(card, { type: "target", shown: true, peekActive: false, reduce: false });
  expect(held.phase).toBe("closed");
  expect(held.inset).toBe(false);
  expect(held.pending).toBe("open");

  const opening = run(held, { type: "painted" });
  expect(opening.phase).toBe("opening");
  expect(opening.inset).toBe(false);
});

it("drops a hover card that never came on screen, and eases away one that did", () => {
  const waiting = run(closed, { type: "target", shown: false, peekActive: true, reduce: false });
  expect(waiting.inset).toBe(true);
  expect(waiting.insetShown).toBe(false);
  const dropped = run(waiting, { type: "target", shown: false, peekActive: false, reduce: false });
  expect(dropped.inset).toBe(false);

  const onScreen = run(waiting, { type: "painted" });
  const leaving = run(onScreen, { type: "target", shown: false, peekActive: false, reduce: false });
  expect(leaving.inset).toBe(true);
  expect(leaving.insetShown).toBe(false);
  expect(run(leaving, { type: "inset-end" }).inset).toBe(false);
});

it("brings the card back if the pointer returns while it is leaving", () => {
  const onScreen = run(
    closed,
    { type: "target", shown: false, peekActive: true, reduce: false },
    { type: "painted" },
  );
  const leaving = run(onScreen, { type: "target", shown: false, peekActive: false, reduce: false });
  const back = run(leaving, { type: "target", shown: false, peekActive: true, reduce: false });
  expect(back.inset).toBe(true);
  expect(back.pending).toBe("show-inset");
  expect(run(back, { type: "painted" }).insetShown).toBe(true);
});

it("reverses a close that is still easing", () => {
  const closing = run(open, { type: "target", shown: false, peekActive: false, reduce: false });
  const opening = run(closing, { type: "target", shown: true, peekActive: false, reduce: false });
  expect(opening.phase).toBe("opening");
  expect(opening.inset).toBe(false);
});

it("ignores a column end that does not belong to the phase in progress", () => {
  expect(run(open, { type: "column-end", reduce: false }).phase).toBe("open");
  expect(run(closed, { type: "inset-end" }).inset).toBe(false);
});
