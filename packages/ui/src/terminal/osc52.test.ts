// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi, type MockInstance } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { ActionableError } from "../errors/actionableError";
import type { Translate } from "../i18n/catalog";
import { osc52ClipboardText, programCopyHandler } from "./osc52";

describe("osc52ClipboardText", () => {
  it("decodes the base64 text as UTF-8, whichever selections it names", () => {
    expect(osc52ClipboardText(`c;${btoa("hello")}`)).toBe("hello");
    expect(osc52ClipboardText(`;${btoa(String.fromCharCode(...new TextEncoder().encode("复制")))}`)).toBe("复制");
  });

  it.each([
    ["a query", "c;?"],
    ["a request to clear", "c;"],
    ["no selection field", btoa("hello")],
    ["text that is not base64", "c;not base64!"],
    ["bytes that are not UTF-8", `c;${btoa("\xff\xfe")}`],
  ])("gives nothing for %s", (_, payload) => {
    expect(osc52ClipboardText(payload)).toBeUndefined();
  });
});

describe("programCopyHandler", () => {
  const t = ((key: string) => key) as Translate;
  let report: MockInstance<typeof errorAlerts.report>;
  beforeEach(() => { report = vi.spyOn(errorAlerts, "report").mockImplementation(() => {}); });
  afterEach(() => { report.mockRestore(); });
  const copyWith = async (writeText: (text: string) => Promise<void>) => {
    programCopyHandler({ writeText }, () => ({ t }))!("text");
    await new Promise((resolve) => setTimeout(resolve));
    return report;
  };

  it("alerts when the clipboard fails, with the clipboard as the way out", async () => {
    await copyWith(() => Promise.reject(new Error("denied")));
    expect(report).toHaveBeenCalledOnce();
    const { error, ...alert } = report.mock.lastCall![0];
    expect(alert).toMatchObject({ message: "terminal.copyFailed", title: "common.copyFailedTitle" });
    expect((error as ActionableError).recovery).toBe("clipboard");
  });

  it("stays quiet when a browser refuses a copy without a user gesture", async () => {
    expect(await copyWith(() => Promise.reject(new DOMException("no gesture", "NotAllowedError")))).not.toHaveBeenCalled();
  });

  it("is absent without a clipboard", () => {
    expect(programCopyHandler(undefined, () => ({ t }))).toBeUndefined();
  });
});
