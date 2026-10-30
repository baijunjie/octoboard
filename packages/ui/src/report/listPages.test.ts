import { expect, it, vi } from "vitest";

import type { Daemon } from "../store";
import { listPages } from "./listPages";

const requester = (reply: () => Promise<unknown> = async () => ({ type: "ack", id: null })) => vi.fn(reply) as unknown as Daemon["request"] & ReturnType<typeof vi.fn>;

it("shares one listing per session and snapshot epoch, in flight or answered", async () => {
  const request = requester();
  await Promise.all([listPages(request, "s", 1), listPages(request, "s", 1)]);
  await listPages(request, "s", 1);
  expect(request).toHaveBeenCalledTimes(1);

  await listPages(request, "other", 1);
  await listPages(request, "s", 2);
  expect(request).toHaveBeenCalledTimes(3);
});

it("lists again after a failure", async () => {
  const failing = requester(async () => {
    throw new Error("offline");
  });
  await expect(listPages(failing, "s", 1)).rejects.toThrow("offline");
  await expect(listPages(failing, "s", 1)).rejects.toThrow("offline");
  expect(failing).toHaveBeenCalledTimes(2);
});
