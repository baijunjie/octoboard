// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { afterFrames } from "./afterFrames";

beforeEach(() => vi.useFakeTimers({ toFake: ["requestAnimationFrame", "cancelAnimationFrame"] }));
afterEach(() => vi.useRealTimers());

const frame = () => vi.advanceTimersToNextFrame();

describe("afterFrames", () => {
  it("runs after the given number of frames, not before", () => {
    const then = vi.fn();
    afterFrames(2, then);
    frame();
    expect(then).not.toHaveBeenCalled();
    frame();
    expect(then).toHaveBeenCalledTimes(1);
  });

  it("does nothing once cancelled", () => {
    const then = vi.fn();
    const cancel = afterFrames(2, then);
    frame();
    cancel();
    frame();
    frame();
    expect(then).not.toHaveBeenCalled();
  });
});
