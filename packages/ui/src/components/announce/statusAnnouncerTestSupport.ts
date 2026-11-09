// Shared by the tests that mount a component holding a status region (`StatusAnnouncer`), the whole
// `App` included, on the real clock.
import { act } from "react";
import { vi } from "vitest";
import { waitOnRealClock } from "../../test/clock";
import { FIRST_WRITE_DELAY_MS } from "./StatusAnnouncer";

/**
 * Waits for a region mounted on the real clock to become ready before a later fake-clock operation. Its first-write
 * timer is armed on the real clock at mount, so advancing a fake clock never fires it.
 */
export async function firstWriteDelayPassed(): Promise<void> {
  if (vi.isFakeTimers()) {
    throw new Error("firstWriteDelayPassed needs the real clock; advance FIRST_WRITE_DELAY_MS on the fake clock");
  }
  await act(() => waitOnRealClock(FIRST_WRITE_DELAY_MS));
}
