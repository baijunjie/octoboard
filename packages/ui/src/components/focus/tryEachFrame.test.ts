// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { tryEachFrame } from "./tryEachFrame";

beforeEach(() => vi.useFakeTimers({ toFake: ["requestAnimationFrame", "cancelAnimationFrame"] }));
afterEach(() => vi.useRealTimers());

const frame = () => vi.advanceTimersToNextFrame();

describe("tryEachFrame", () => {
  it("tries on the next frame and stops once the attempt says it is done", () => {
    const attempt = vi.fn().mockReturnValueOnce(false).mockReturnValueOnce(true);
    tryEachFrame(attempt, 5);
    expect(attempt).not.toHaveBeenCalled();
    frame();
    frame();
    frame();
    expect(attempt).toHaveBeenCalledTimes(2);
  });

  it("gives up after the retries", () => {
    const attempt = vi.fn(() => false);
    tryEachFrame(attempt, 2);
    for (let i = 0; i < 6; i++) frame();
    expect(attempt).toHaveBeenCalledTimes(3);
  });

  it("does nothing once cancelled", () => {
    const attempt = vi.fn(() => false);
    const cancel = tryEachFrame(attempt, 2);
    frame();
    cancel();
    frame();
    frame();
    expect(attempt).toHaveBeenCalledTimes(1);
  });
});
