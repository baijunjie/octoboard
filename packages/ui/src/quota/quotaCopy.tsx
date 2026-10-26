import React from "react";

import { InlineReference } from "../components/InlineReference";
import type { Translate } from "../i18n/catalog";
import { Message } from "../i18n/react";
import type { QuotaWindow } from "../protocol";
import { quotaResetsIn, quotaWindowLabel, quotaWindowPhrase, type QuotaNaming } from "./quotaFormat";

/** A quota window referenced in copy, keeping model values apart from its translated label. */
export function QuotaWindowName({ naming, window, t }: { naming: QuotaNaming; window: QuotaWindow; t: Translate }): React.ReactElement {
  return window.kind === "weekly_model" && window.label
    ? <Message id="quota.window.weeklyModel" params={{ model: <InlineReference monospace wrap>{window.label}</InlineReference> }} />
    : <>{quotaWindowLabel(t, naming, window)}</>;
}

/** Rich visible counterpart to the plain `quotaUsedUpSentence`: one sentence saying the account cannot be used for
 * now and why, for the error text under the field that holds the account and for the content of an error alert
 * that refuses the account. */
export function quotaUsedUpMessage(t: Translate, naming: QuotaNaming, account: string, window: QuotaWindow, now: number): React.ReactNode {
  const resets = quotaResetsIn(t, window, now);
  const quota = window.kind === "weekly_model" && window.label
    ? <Message id="quota.windowPhrase.weeklyModel" params={{ model: <InlineReference monospace wrap>{window.label}</InlineReference> }} />
    : quotaWindowPhrase(t, naming, window);
  const name = <InlineReference dir="auto" wrap>{account}</InlineReference>;
  return resets
    ? <Message id="quota.accountUsedUp.resets" params={{ account: name, quota, resets }} />
    : <Message id="quota.accountUsedUp" params={{ account: name, quota }} />;
}
