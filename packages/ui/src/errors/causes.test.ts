import { expect, it } from "vitest";

import { accountTrashFailedCause } from "./causes";

it.each(["unanswered", "unreadable", "other_unlooked", "signing_in"])("reads the account trash cause %s as itself", (cause) => {
  expect(accountTrashFailedCause(cause)).toBe(cause);
});

it.each([undefined, "", "other", "from_a_newer_daemon", "SIGNING_IN"])("reads %j, no cause or one the client does not know, as none", (cause) => {
  expect(accountTrashFailedCause(cause)).toBeUndefined();
});
