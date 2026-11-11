// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";

import { tryEachFrame } from "./tryEachFrame";
import { fakeTimers } from "../../test/clock";

beforeEach(() => {
  fakeTimers({ toFake: ["requestAnimationFrame", "cancelAnimationFrame"] });
});

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

  it("calls onGiveUp once the retries are spent, and not when the attempt is done", () => {
    const giveUp = vi.fn();
    tryEachFrame(() => false, 1, giveUp);
    frame();
    expect(giveUp).not.toHaveBeenCalled();
    frame();
    frame();
    expect(giveUp).toHaveBeenCalledTimes(1);
    const done = vi.fn();
    tryEachFrame(() => true, 1, done);
    for (let i = 0; i < 3; i++) frame();
    expect(done).not.toHaveBeenCalled();
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
