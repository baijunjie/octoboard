import { describe, expect, it } from "vitest";

import { CodedError } from "./codedError";
import { codeRecord, isDaemonRefusal, messageFacets } from "./codes";

describe("codeRecord", () => {
  it("knows no code the UI has nothing to say about, and no inherited property", () => {
    expect(codeRecord("a_code_from_a_newer_daemon")).toBeUndefined();
    expect(codeRecord("constructor")).toBeUndefined();
    expect(codeRecord("__proto__")).toBeUndefined();
  });

  // `daemonCodes.test.ts` holds the registry to the catalogs and to the daemon; these are the exceptions its rules
  // leave to a record, on purpose.
  it("leaves a sharing failure's diagnostic block and opening sentence to the one code that has them", () => {
    // A sharing failure lists its entries, but only `account_sharing_failed` has a diagnostic block of its own, and
    // neither it nor `project_branches_not_determined` has a `.summary` to leave the diagnostic out of.
    expect(codeRecord("account_sharing_failed")).toMatchObject({ sharingItems: true, detailBlock: true, summaryForDetail: false });
    expect(codeRecord("account_sharing_repair_failed")).toMatchObject({ sharingItems: true });
    expect(codeRecord("account_sharing_repair_failed")?.detailBlock).toBeUndefined();
    expect(codeRecord("project_branches_not_determined")).toMatchObject({ detailBlock: true, summaryForDetail: false });
  });

  it("words the shell-environment timeout by its own details, not by the recovery line", () => {
    expect(codeRecord("shell_environment_timeout")?.recoveryLine).toBeUndefined();
  });
});

describe("messageFacets", () => {
  it("reads a code's own params, or those of the variant a message is worded under", () => {
    expect(messageFacets("path_not_found")?.codeParams).toEqual(["path"]);
    expect(messageFacets("relocation_failed.kept_staging")?.codeParams).toEqual(["path", "kept_in"]);
    expect(messageFacets("terminal_input_lost.shell")?.nameParams).toEqual(["shell_terminal"]);
    // The variants of `terminal_input_lost` carry the names; the code itself, worded under neither, shows none.
    expect(messageFacets("terminal_input_lost")?.nameParams).toBeUndefined();
  });

  it("knows nothing of a variant or a code it does not list", () => {
    expect(messageFacets("path_not_found.other")).toBeUndefined();
    expect(messageFacets("a_code_from_a_newer_daemon.variant")).toBeUndefined();
  });
});

describe("isDaemonRefusal", () => {
  it("is a coded failure the daemon sends, a code this client does not know yet included", () => {
    expect(isDaemonRefusal(new CodedError("x", "daemon_shutting_down", {}))).toBe(true);
    expect(isDaemonRefusal(new CodedError("x", "a_code_from_a_newer_daemon", {}))).toBe(true);
  });

  it("is not the client's own failure to get an answer, nor anything that is not a coded failure", () => {
    expect(isDaemonRefusal(new CodedError("x", "connection_closed", {}))).toBe(false);
    expect(isDaemonRefusal(new CodedError("x", "request_timed_out", {}))).toBe(false);
    expect(isDaemonRefusal(new Error("x"))).toBe(false);
    expect(isDaemonRefusal("daemon_shutting_down")).toBe(false);
  });
});
