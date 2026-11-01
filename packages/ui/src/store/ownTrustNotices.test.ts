import { expect, it } from "vitest";

import type { Event } from "../protocol";
import { createOwnTrustNotices } from "./ownTrustNotices";

type Notice = Extract<Event, { type: "session_notice" }>;

const notice = (session: string, code = "trust_answer_failed"): Notice => ({ type: "session_notice", session, code, params: {}, message: "raw" });

function setup() {
  const shown: Notice[] = [];
  const own = createOwnTrustNotices((held) => shown.push(held));
  // What the connection does with a notice: claimed ones are not shown now, the rest are.
  const receive = (n: Notice) => {
    if (!own.claim(n)) shown.push(n);
  };
  return { own, shown, receive };
}

it("discards the held notice a go-ahead's rejection matches", () => {
  const { own, shown, receive } = setup();
  const settle = own.begin("s");
  receive(notice("s"));
  settle("trust_answer_failed");
  expect(shown).toEqual([]);
  receive(notice("s"));
  expect(shown).toHaveLength(1);
});

it.each(["trust_answer_failed", "trust_not_carried_over"])("swallows the %s notice that follows the rejection, once", (code) => {
  const { own, shown, receive } = setup();
  own.begin("s")(code);
  receive(notice("s", code));
  expect(shown).toEqual([]);
  receive(notice("s", code));
  expect(shown).toHaveLength(1);
});

it("emits a notice held in flight when the go-ahead succeeds or fails without a code", () => {
  const { own, shown, receive } = setup();
  const settle = own.begin("s");
  receive(notice("s", "trust_not_carried_over"));
  expect(shown).toEqual([]);
  settle();
  expect(shown).toEqual([notice("s", "trust_not_carried_over")]);
});

it("emits a notice held in flight when the go-ahead fails with another code", () => {
  const { own, shown, receive } = setup();
  const settle = own.begin("s");
  receive(notice("s", "trust_not_carried_over"));
  settle("trust_answer_failed");
  expect(shown).toEqual([notice("s", "trust_not_carried_over")]);
  // The rejection had no notice of its own yet, so it left a token for it.
  receive(notice("s", "trust_answer_failed"));
  expect(shown).toHaveLength(1);
});

it("emits a notice held in flight when the go-ahead is not waiting any more", () => {
  const { own, shown, receive } = setup();
  const settle = own.begin("s");
  receive(notice("s", "trust_not_carried_over"));
  settle("trust_not_waiting");
  expect(shown).toEqual([notice("s", "trust_not_carried_over")]);
  receive(notice("s", "trust_answer_failed"));
  expect(shown).toHaveLength(2);
});

it("discards only one of two held notices of the matching code", () => {
  const { own, shown, receive } = setup();
  const settle = own.begin("s");
  receive(notice("s"));
  receive(notice("s"));
  settle("trust_answer_failed");
  expect(shown).toHaveLength(1);
});

it("shows a notice for another session, another code, or without a go-ahead of its own", () => {
  const { own, shown, receive } = setup();
  const settle = own.begin("s");
  receive(notice("other"));
  receive(notice("s", "account_sharing_repair_failed"));
  settle("trust_answer_failed");
  receive(notice("other"));
  expect(shown).toHaveLength(3);
});

it("forgets a held token at a snapshot, so a later unrelated notice shows", () => {
  const { own, shown, receive } = setup();
  own.begin("s")("trust_answer_failed");
  own.reset();
  receive(notice("s"));
  expect(shown).toHaveLength(1);
});

it("adds no token when a snapshot arrived between the send and the reply", () => {
  const { own, shown, receive } = setup();
  const settle = own.begin("s");
  own.reset();
  settle("trust_answer_failed");
  receive(notice("s"));
  expect(shown).toHaveLength(1);
});

it("keeps a go-ahead in flight and a held notice across a snapshot", () => {
  const { own, shown, receive } = setup();
  const settle = own.begin("s");
  receive(notice("s", "trust_not_carried_over"));
  own.reset();
  receive(notice("s", "trust_answer_failed"));
  expect(shown).toEqual([]);
  settle("trust_answer_failed");
  expect(shown).toEqual([notice("s", "trust_not_carried_over")]);
});

it("holds notices until the last of two concurrent go-aheads for a session settles", () => {
  const { own, shown, receive } = setup();
  const first = own.begin("s");
  const second = own.begin("s");
  receive(notice("s", "trust_not_carried_over"));
  receive(notice("s", "trust_answer_failed"));
  first("trust_answer_failed");
  expect(shown).toEqual([]);
  second();
  expect(shown).toEqual([notice("s", "trust_not_carried_over")]);
});

it("lets a token swallow its notice before a go-ahead still in flight holds it", () => {
  const { own, shown, receive } = setup();
  const first = own.begin("s");
  const second = own.begin("s");
  first("trust_answer_failed");
  receive(notice("s"));
  second();
  expect(shown).toEqual([]);
  receive(notice("s"));
  expect(shown).toHaveLength(1);
});
