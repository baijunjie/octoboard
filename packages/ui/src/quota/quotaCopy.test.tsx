// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { currentLanguage, setLanguage, t } from "../i18n/language";
import type { QuotaWindow } from "../protocol";
import { quotaUsedUpMessage, QuotaWindowName } from "./quotaCopy";
import { quotaUsedUpSentence, quotaWindowLabel } from "./quotaFormat";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it.each(["en", "zh-Hans", "ar"] as const)("keeps the rich used-up sentence the plain one in words (%s)", (language) => {
  const before = currentLanguage();
  setLanguage(language);
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); setLanguage(before); });
  const name = "العمل Work/中文";
  for (const kind of ["session", "weekly", "credit_period", "weekly_model"] as const) {
    const window: QuotaWindow = { kind, label: kind === "weekly_model" ? "model/中文" : null, used_percent: 100, resets_at: 7200000, window_minutes: null };
    const naming = { agent: "claude" as const, windows: [window] };
    act(() => root.render(<p>{quotaUsedUpMessage(t, naming, name, window, 0)}</p>));
    const bare = (text: string) => text.replace(/[\u2068\u2069]/g, "");
    expect(container.textContent).toBe(bare(quotaUsedUpSentence(t, naming, name, window, 0)));
    expect(container.querySelector("[data-inline-reference]")?.textContent).toBe(name);
    expect(container.querySelector("[data-inline-reference]")?.getAttribute("dir")).toBe("auto");
    expect(quotaUsedUpSentence(t, naming, name, window, 0)).not.toMatch(/<[^>]+>/);
    act(() => root.render(<QuotaWindowName naming={naming} window={window} t={t} />));
    expect(container.textContent).toBe(bare(quotaWindowLabel(t, naming, window)));
    expect(container.querySelectorAll("code")).toHaveLength(Number(kind === "weekly_model"));
  }
});
