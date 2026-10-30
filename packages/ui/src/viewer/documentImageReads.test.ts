import { expect, it, vi } from "vitest";

import type { Event, RequestBody } from "../protocol";
import { readDocumentImage } from "./documentImageReads";

function daemon() {
  const sent: { path: string; slot?: string; answer: (event: Event) => void }[] = [];
  const request = vi.fn(
    (body: RequestBody) =>
      new Promise<Event>((resolve) => {
        const { path, slot } = body as { path: string; slot?: string };
        sent.push({ path, slot, answer: resolve });
      }),
  );
  return { sent, request };
}

const file = (path: string) => ({ type: "project_file", path }) as unknown as Event;
const settle = () => new Promise((resolve) => setTimeout(resolve));

it("reads one image at a time across the window, in order", async () => {
  const { sent, request } = daemon();
  const first = readDocumentImage(request, "p", "a.png", new AbortController().signal);
  const second = readDocumentImage(request, "q", "b.png", new AbortController().signal);
  expect(sent.map((read) => read.path)).toEqual(["a.png"]);
  sent[0]!.answer(file("a.png"));
  await expect(first).resolves.toEqual(file("a.png"));
  await settle();
  expect(sent.map((read) => [read.path, read.slot])).toEqual([["a.png", "document-image"], ["b.png", "document-image"]]);
  sent[1]!.answer(file("b.png"));
  await expect(second).resolves.toEqual(file("b.png"));
});

// The read sent next goes in the same slot, so the daemon cancels the read let go of.
it("never sends a read let go of while it waits, and gives the place of one let go of while out to the next", async () => {
  const { sent, request } = daemon();
  const out = new AbortController();
  const queued = new AbortController();
  const outRead = readDocumentImage(request, "p", "a.png", out.signal);
  const queuedRead = readDocumentImage(request, "p", "b.png", queued.signal);
  const next = readDocumentImage(request, "p", "c.png", new AbortController().signal);
  queued.abort();
  await expect(queuedRead).rejects.toBe(queued.signal.reason);
  out.abort();
  await expect(outRead).rejects.toBe(out.signal.reason);
  expect(sent.map((read) => read.path)).toEqual(["a.png", "c.png"]);
  // The answer the read let go of still gets does not free a second place.
  sent[0]!.answer(file("a.png"));
  await settle();
  readDocumentImage(request, "p", "d.png", new AbortController().signal);
  expect(sent.map((read) => read.path)).toEqual(["a.png", "c.png"]);
  sent[1]!.answer(file("c.png"));
  await expect(next).resolves.toEqual(file("c.png"));
  await settle();
  expect(sent.map((read) => read.path)).toEqual(["a.png", "c.png", "d.png"]);
  sent[2]!.answer(file("d.png"));
});
