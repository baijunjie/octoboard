import { expect, it } from "vitest";

import { accountTrashFailedCause, projectTrashFailedCause, sharingCause } from "./causes";

it.each(["unanswered", "unreadable", "other_unlooked", "signing_in"])("reads the account trash cause %s as itself", (cause) => {
  expect(accountTrashFailedCause(cause)).toBe(cause);
});

it.each([undefined, "", "other", "from_a_newer_daemon", "SIGNING_IN"])("reads %j, no cause or one the client does not know, as none", (cause) => {
  expect(accountTrashFailedCause(cause)).toBeUndefined();
});

it.each(["unanswered", "unreadable"])("reads the project trash cause %s as itself", (cause) => {
  expect(projectTrashFailedCause(cause)).toBe(cause);
});

it.each([undefined, "", "other", "other_unlooked", "signing_in", "from_a_newer_daemon", "UNANSWERED"])("reads %j, no project trash cause or one the client does not know, as none", (cause) => {
  expect(projectTrashFailedCause(cause)).toBeUndefined();
});

it.each(["permission_denied", "busy", "not_found", "no_space", "conflict", "default_moved", "other"])("reads the sharing cause %s as itself", (cause) => {
  expect(sharingCause(cause)).toBe(cause);
});

it.each([undefined, "", "from_a_newer_daemon", "BUSY"])("reads %j, no sharing cause or one the client does not know, as other", (cause) => {
  expect(sharingCause(cause)).toBe("other");
});
