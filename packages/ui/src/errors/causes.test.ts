import { expect, it } from "vitest";

import { accountTrashFailedCause, projectTrashFailedCause, sessionDidNotStopCause, sessionGitHooksUnavailableCause, sharingCause, shellEnvironmentUnavailableCause } from "./causes";

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

it("reads the session did not stop cause open_in_app as itself", () => {
  expect(sessionDidNotStopCause("open_in_app")).toBe("open_in_app");
});

it.each([undefined, "", "switch", "from_a_newer_daemon", "OPEN_IN_APP"])("reads %j, no cause or one the client does not know, as none, so the code is worded as a switch's", (cause) => {
  expect(sessionDidNotStopCause(cause)).toBeUndefined();
});

it.each(["earlier_capture", "daemon"])("reads the shell environment cause %s as itself", (cause) => {
  expect(shellEnvironmentUnavailableCause(cause)).toBe(cause);
});

it.each([undefined, "", "other", "from_a_newer_daemon", "DAEMON"])("reads %j, no shell environment cause or one the client does not know, as none", (cause) => {
  expect(shellEnvironmentUnavailableCause(cause)).toBeUndefined();
});

it.each(["git_too_old", "user_configuration", "other"])("reads the session git hooks cause %s as itself", (cause) => {
  expect(sessionGitHooksUnavailableCause(cause)).toBe(cause);
});

it.each([undefined, "", "from_a_newer_daemon", "GIT_TOO_OLD"])("reads %j, no session git hooks cause or one the client does not know, as other", (cause) => {
  expect(sessionGitHooksUnavailableCause(cause)).toBe("other");
});
