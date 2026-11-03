import { expect, it, vi } from "vitest";

import { createBrowseLane } from "./lane";

/** A request the test answers itself. */
function deferred() {
  let resolve!: (value: string) => void;
  const promise = new Promise<string>((done) => (resolve = done));
  return { promise, resolve };
}
const settle = () => new Promise((resolve) => setTimeout(resolve));

it("has at most its places out, sending the next in order as one settles", async () => {
  const lane = createBrowseLane(2);
  const sent: string[] = [];
  const requests = ["a", "b", "c"].map((name) => {
    const request = deferred();
    const answer = lane.send(() => (sent.push(name), request.promise));
    return { request, answer };
  });
  expect(sent).toEqual(["a", "b"]);
  requests[0]!.request.resolve("A");
  await expect(requests[0]!.answer).resolves.toBe("A");
  await settle();
  expect(sent).toEqual(["a", "b", "c"]);
});

it("frees the place of a request its caller skips when its turn comes", async () => {
  const lane = createBrowseLane(1);
  const first = deferred();
  lane.send(() => first.promise);
  const skipped = lane.send(() => undefined);
  const sent: string[] = [];
  lane.send(() => (sent.push("next"), deferred().promise));
  first.resolve("A");
  await expect(skipped).resolves.toBeUndefined();
  await settle();
  expect(sent).toEqual(["next"]);
});

it("sends a request replacing one in its slot at once in a replacing lane, and queues it otherwise", () => {
  const replacing = createBrowseLane(1, { replacing: true });
  const plain = createBrowseLane(1);
  for (const lane of [replacing, plain]) lane.send(() => deferred().promise, { slot: "s" });
  const sent: string[] = [];
  replacing.send(() => (sent.push("replacing"), deferred().promise), { slot: "s" });
  plain.send(() => (sent.push("plain"), deferred().promise), { slot: "s" });
  // A request in another slot waits either way.
  replacing.send(() => (sent.push("other"), deferred().promise), { slot: "t" });
  expect(sent).toEqual(["replacing"]);
});

it("drops a request let go of while it waits, and in a lane of one slot gives the place of one let go of while out to the next", async () => {
  const lane = createBrowseLane(1, { slot: "s" });
  const sent: string[] = [];
  const out = new AbortController();
  const queued = new AbortController();
  const outRequest = deferred();
  const outAnswer = lane.send(() => (sent.push("out"), outRequest.promise), { signal: out.signal });
  const queuedAnswer = lane.send(() => (sent.push("queued"), deferred().promise), { signal: queued.signal });
  lane.send(() => (sent.push("next"), deferred().promise));
  queued.abort();
  await expect(queuedAnswer).rejects.toBe(queued.signal.reason);
  out.abort();
  await expect(outAnswer).rejects.toBe(out.signal.reason);
  expect(sent).toEqual(["out", "next"]);
  // The answer the request let go of still gets does not free a second place.
  outRequest.resolve("late");
  await settle();
  lane.send(() => (sent.push("after"), deferred().promise));
  expect(sent).toEqual(["out", "next"]);
});

it("keeps the place of a request let go of while out until it settles, in a lane that is not of one slot", async () => {
  const lane = createBrowseLane(1);
  const sent: string[] = [];
  const out = new AbortController();
  const outRequest = deferred();
  const outAnswer = lane.send(() => (sent.push("out"), outRequest.promise), { signal: out.signal });
  lane.send(() => (sent.push("next"), deferred().promise));
  out.abort();
  await expect(outAnswer).rejects.toBe(out.signal.reason);
  await settle();
  expect(sent).toEqual(["out"]);
  outRequest.resolve("late");
  await settle();
  expect(sent).toEqual(["out", "next"]);
});

it("refuses a slot of its own for a request in a lane of one slot", async () => {
  const lane = createBrowseLane(1, { slot: "s" });
  await expect(lane.send(() => deferred().promise, { slot: "t" })).rejects.toThrow("names no slot of its own");
});

it("frees the place of a request whose send throws, and rejects with what it threw", async () => {
  const lane = createBrowseLane(1);
  const failure = new Error("sent badly");
  await expect(lane.send(() => { throw failure; })).rejects.toBe(failure);
  const sent: string[] = [];
  lane.send(() => (sent.push("next"), deferred().promise));
  expect(sent).toEqual(["next"]);
});

it("takes its abort listener off the signal once the request settles", async () => {
  const lane = createBrowseLane(1);
  const controller = new AbortController();
  const remove = vi.spyOn(controller.signal, "removeEventListener");
  await expect(lane.send(() => Promise.resolve("done"), { signal: controller.signal })).resolves.toBe("done");
  await settle();
  expect(remove).toHaveBeenCalledWith("abort", expect.any(Function));
});
