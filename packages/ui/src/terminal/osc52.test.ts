// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";

import { ActionableError } from "../errorRecovery";
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
  const copyWith = async (writeText: (text: string) => Promise<void>) => {
    const toastError = vi.fn();
    programCopyHandler({ writeText }, () => ({ t, toastError }))!("text");
    await new Promise((resolve) => setTimeout(resolve));
    return toastError;
  };

  it("alerts when the clipboard fails, with the clipboard as the way out", async () => {
    const toastError = await copyWith(() => Promise.reject(new Error("denied")));
    expect(toastError).toHaveBeenCalledOnce();
    const error = toastError.mock.calls[0][0] as ActionableError;
    expect(error.message).toBe("terminal.copyFailed");
    expect(error.recovery).toBe("clipboard");
  });

  it("stays quiet when a browser refuses a copy without a user gesture", async () => {
    expect(await copyWith(() => Promise.reject(new DOMException("no gesture", "NotAllowedError")))).not.toHaveBeenCalled();
  });

  it("is absent without a clipboard", () => {
    expect(programCopyHandler(undefined, () => ({ t, toastError: () => {} }))).toBeUndefined();
  });
});
