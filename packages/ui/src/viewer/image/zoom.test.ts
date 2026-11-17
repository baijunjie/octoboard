import { describe, expect, it } from "vitest";

import { FIT, MAX_SCALE, anchoredScroll, canStep, fitScale, scaleOf, stepScale, wheelFactor, zoomBounds, zoomKey, zoomTo } from "./zoom";

const LEVELS = [0.1, 0.25, 0.33, 0.5, 0.67, 0.75, 1, 1.25, 1.5, 2, 3, 4, 6, 8, 12, 16, 24, 32];

/** The scale a step lands on, whichever mode it is expressed in. */
const landed = (scale: number, direction: 1 | -1, bounds: { fit: number; max: number }): number => scaleOf(stepScale(scale, direction, bounds), bounds.fit);

describe("fitScale", () => {
  it("scales a large image down by the tighter side", () => {
    expect(fitScale({ width: 2000, height: 1000 }, { width: 1000, height: 1000 })).toBeCloseTo(0.5);
    expect(fitScale({ width: 1000, height: 2000 }, { width: 1000, height: 1000 })).toBeCloseTo(0.5);
    expect(fitScale({ width: 4000, height: 1000 }, { width: 1000, height: 500 })).toBeCloseTo(0.25);
  });

  it("never scales an image above its actual size", () => {
    expect(fitScale({ width: 100, height: 50 }, { width: 1000, height: 1000 })).toBe(1);
    expect(fitScale({ width: 1000, height: 1000 }, { width: 1000, height: 1000 })).toBe(1);
  });
});

describe("zoomBounds", () => {
  it("uses the fitted scale as the smallest and 32x as the largest", () => {
    const bounds = zoomBounds({ width: 2000, height: 1000 }, { width: 1000, height: 1000 });
    expect(bounds.fit).toBeCloseTo(0.5);
    expect(bounds.max).toBe(MAX_SCALE);
    expect(MAX_SCALE).toBe(32);
  });

  it("caps the largest scale so the longest drawn side stays within 65536 px", () => {
    const bounds = zoomBounds({ width: 8192, height: 1000 }, { width: 1000, height: 1000 });
    expect(bounds.max).toBeCloseTo(65536 / 8192);
    expect(zoomBounds({ width: 1000, height: 16384 }, { width: 1000, height: 1000 }).max).toBeCloseTo(4);
  });

  it("never puts the largest scale below actual size", () => {
    const bounds = zoomBounds({ width: 200000, height: 100 }, { width: 1000, height: 1000 });
    expect(bounds.max).toBe(1);
    expect(bounds.fit).toBeLessThan(1);
  });
});

describe("scaleOf", () => {
  it("is the fitted scale while fitted and the chosen scale otherwise", () => {
    expect(scaleOf(FIT, 0.4)).toBe(0.4);
    expect(scaleOf({ mode: "scale", scale: 3 }, 0.4)).toBe(3);
  });
});

describe("zoomTo", () => {
  const bounds = { fit: 0.5, max: 8 };

  it("holds the scale within the bounds", () => {
    expect(zoomTo(100, bounds)).toEqual({ mode: "scale", scale: 8 });
    expect(zoomTo(2, bounds)).toEqual({ mode: "scale", scale: 2 });
  });

  it("returns to fitted mode on landing on the fitted scale, and on anything below it", () => {
    expect(zoomTo(0.5, bounds)).toEqual(FIT);
    expect(zoomTo(0.5 * (1 + 1e-9), bounds)).toEqual(FIT);
    expect(zoomTo(0.01, bounds)).toEqual(FIT);
  });
});

describe("stepScale and canStep", () => {
  it("steps through the fixed levels, from level to level", () => {
    const bounds = { fit: 0.05, max: 32 };
    for (let index = 0; index < LEVELS.length - 1; index += 1) {
      expect(landed(LEVELS[index], 1, bounds)).toBe(LEVELS[index + 1]);
      expect(landed(LEVELS[index + 1], -1, bounds)).toBe(LEVELS[index]);
    }
  });

  it("includes the fitted scale as a stop, and returns to fitted mode on landing on it", () => {
    const bounds = { fit: 0.4, max: 32 };
    expect(stepScale(0.5, -1, bounds)).toEqual(FIT);
    expect(landed(0.25, 1, bounds)).toBe(0.4);
    expect(landed(0.4, 1, bounds)).toBe(0.5);
  });

  it("includes the largest scale as a stop, and a step that would pass the bounds lands on them", () => {
    const bounds = { fit: 1, max: 6.5536 };
    expect(landed(4, 1, bounds)).toBe(6);
    expect(landed(6, 1, bounds)).toBeCloseTo(6.5536);
    expect(landed(5.9, 1, { fit: 1, max: 6 })).toBe(6);
    expect(landed(0.34, -1, { fit: 0.31, max: 32 })).toBe(0.31);
  });

  it("passes over a stop less than a factor of 1.1 away, so a press always visibly zooms", () => {
    // Fitted at 31%, the 33% level beside it is skipped.
    const bounds = { fit: 0.31, max: 32 };
    expect(landed(0.31, 1, bounds)).toBe(0.5);
    expect(landed(0.5, -1, bounds)).toBe(0.33);
    expect(landed(0.33, -1, bounds)).toBe(0.31);
    // A stop exactly as far as the factor still steps: 0.67 to 0.75 is more than 1.1x.
    expect(landed(0.67, 1, { fit: 0.1, max: 32 })).toBe(0.75);
    expect(landed(0.75, -1, { fit: 0.1, max: 32 })).toBe(0.67);
    // Zooming out from the largest scale passes the level just under it.
    expect(landed(6.5536, -1, { fit: 1, max: 6.5536 })).toBe(4);
    // Whatever the scale, a step that can be taken changes it by at least the factor, short of the bounds.
    const open = { fit: 0.05, max: 32 };
    for (const scale of [0.06, 0.1, 0.26, 0.34, 0.66, 0.7, 1, 1.1, 1.3, 5, 7, 20]) {
      expect(landed(scale, 1, open) / scale).toBeGreaterThanOrEqual(1.1 - 1e-9);
      expect(scale / landed(scale, -1, open)).toBeGreaterThanOrEqual(1.1 - 1e-9);
    }
  });

  it("never goes past the bounds", () => {
    const bounds = { fit: 0.3, max: 4 };
    expect(landed(4, 1, bounds)).toBe(4);
    expect(stepScale(0.3, -1, bounds)).toEqual(FIT);
    expect(landed(0.5, 1, { fit: 1, max: 1 })).toBe(1);
  });

  it("cannot step further out than the fitted scale, nor further in than the largest", () => {
    const bounds = { fit: 0.5, max: 8 };
    expect(canStep(0.5, -1, bounds)).toBe(false);
    expect(canStep(0.5, 1, bounds)).toBe(true);
    expect(canStep(8, 1, bounds)).toBe(false);
    expect(canStep(8, -1, bounds)).toBe(true);
    expect(canStep(2, 1, bounds)).toBe(true);
    expect(canStep(2, -1, bounds)).toBe(true);
  });

  it("treats a scale within rounding of a bound as on it", () => {
    const bounds = { fit: 0.5, max: 8 };
    expect(canStep(0.5 * (1 + 1e-6), -1, bounds)).toBe(false);
    expect(canStep(8 * (1 - 1e-6), 1, bounds)).toBe(false);
  });

  it("always moves when it can step", () => {
    const bounds = { fit: 0.37, max: 20 };
    for (const scale of [0.37, 0.5, 1, 1.2, 7, 20]) {
      if (canStep(scale, 1, bounds)) expect(landed(scale, 1, bounds)).toBeGreaterThan(scale);
      if (canStep(scale, -1, bounds)) expect(landed(scale, -1, bounds)).toBeLessThan(scale);
    }
  });
});

describe("wheelFactor", () => {
  it("is exp(-delta * 0.01) for pixel deltas", () => {
    expect(wheelFactor({ deltaY: 10, deltaMode: 0 })).toBeCloseTo(Math.exp(-0.1));
    expect(wheelFactor({ deltaY: -10, deltaMode: 0 })).toBeCloseTo(Math.exp(0.1));
    expect(wheelFactor({ deltaY: 0, deltaMode: 0 })).toBe(1);
  });

  it("caps each event's delta at 20 pixels either way", () => {
    expect(wheelFactor({ deltaY: 500, deltaMode: 0 })).toBeCloseTo(Math.exp(-0.2));
    expect(wheelFactor({ deltaY: -500, deltaMode: 0 })).toBeCloseTo(Math.exp(0.2));
  });

  it("reads a line delta as 16 pixels and a page delta as 400 before capping", () => {
    expect(wheelFactor({ deltaY: 1, deltaMode: 1 })).toBeCloseTo(Math.exp(-0.16));
    expect(wheelFactor({ deltaY: 5, deltaMode: 1 })).toBeCloseTo(Math.exp(-0.2));
    expect(wheelFactor({ deltaY: -1, deltaMode: 2 })).toBeCloseTo(Math.exp(0.2));
  });
});

describe("anchoredScroll", () => {
  it("keeps the image point under the anchor where it was", () => {
    const args = {
      scroll: { left: 100, top: 40 },
      anchor: { x: 300, y: 200 },
      previous: 2,
      next: 4,
      imageOffsetBefore: { left: 0, top: 0 },
      imageOffsetAfter: { left: 0, top: 0 },
    };
    const next = anchoredScroll(args);
    // Image point under the anchor: ((100 + 300) / 2, (40 + 200) / 2) = (200, 120); at 4x it sits at (800, 480).
    expect(next.left).toBeCloseTo(800 - 300);
    expect(next.top).toBeCloseTo(480 - 200);
    expect((next.left + 300) / 4).toBeCloseTo((100 + 300) / 2);
    expect((next.top + 200) / 4).toBeCloseTo((40 + 200) / 2);
  });

  it("accounts for the image's offset in the scroller before and after", () => {
    const next = anchoredScroll({
      scroll: { left: 0, top: 0 },
      anchor: { x: 250, y: 150 },
      previous: 1,
      next: 3,
      imageOffsetBefore: { left: 100, top: 50 },
      imageOffsetAfter: { left: 10, top: 0 },
    });
    // Image point: (150, 100); at 3x: (450, 300), plus the new offset, minus the anchor.
    expect(next.left).toBeCloseTo(450 + 10 - 250);
    expect(next.top).toBeCloseTo(300 + 0 - 150);
  });

  it("does not move the scroll when the scale is unchanged", () => {
    const next = anchoredScroll({
      scroll: { left: 30, top: 70 },
      anchor: { x: 10, y: 20 },
      previous: 1.5,
      next: 1.5,
      imageOffsetBefore: { left: 0, top: 0 },
      imageOffsetAfter: { left: 0, top: 0 },
    });
    expect(next.left).toBeCloseTo(30);
    expect(next.top).toBeCloseTo(70);
  });
});

describe("zoomKey", () => {
  const key = (key: string, code = "", altKey = false) => zoomKey({ key, code, altKey });

  it("zooms in on + and = (including the numpad plus and the Equal key)", () => {
    expect(key("+", "Equal")).toBe("in");
    expect(key("=", "Equal")).toBe("in");
    expect(key("+", "NumpadAdd")).toBe("in");
    expect(key("Dead", "Equal")).toBe("in");
  });

  it("zooms out on - (including the numpad minus and the Minus key)", () => {
    expect(key("-", "Minus")).toBe("out");
    expect(key("-", "NumpadSubtract")).toBe("out");
    expect(key("Dead", "Minus")).toBe("out");
  });

  it("returns to fitted on 0, on the digit row or the numpad", () => {
    expect(key("0", "Digit0")).toBe("fit");
    expect(key("0", "Numpad0")).toBe("fit");
    expect(key("Dead", "Digit0")).toBe("fit");
  });

  it("ignores a key held with Alt", () => {
    expect(key("+", "Equal", true)).toBeUndefined();
    expect(key("-", "Minus", true)).toBeUndefined();
    expect(key("0", "Digit0", true)).toBeUndefined();
  });

  it("ignores other keys", () => {
    expect(key("a", "KeyA")).toBeUndefined();
    expect(key("1", "Digit1")).toBeUndefined();
    expect(key("Enter", "Enter")).toBeUndefined();
  });
});
