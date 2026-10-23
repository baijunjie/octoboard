import React from "react";

import { InlineReference } from "../components/InlineReference";
import type { Translate } from "../i18n/catalog";
import { Message } from "../i18n/react";
import type { QuotaWindow } from "../protocol";
import { quotaResetsIn, quotaUsedUpAlert, quotaWindowLabel, type QuotaNaming } from "./quotaFormat";

/** A quota window referenced in copy, keeping model values apart from its translated label. */
export function QuotaWindowName({ naming, window, t }: { naming: QuotaNaming; window: QuotaWindow; t: Translate }): React.ReactElement {
  return window.kind === "weekly_model" && window.label
    ? <Message id="quota.window.weeklyModel" params={{ model: <InlineReference monospace wrap>{window.label}</InlineReference> }} />
    : <>{quotaWindowLabel(t, naming, window)}</>;
}

/** Rich visible counterpart to the plain inline-alert and native sentence APIs. */
export function quotaUsedUpCopy(t: Translate, naming: QuotaNaming, account: string, window: QuotaWindow, now: number): { title: React.ReactNode; description: React.ReactNode } {
  const plain = quotaUsedUpAlert(t, naming, account, window, now);
  const resets = quotaResetsIn(t, window, now);
  const quota = window.kind === "weekly_model" && window.label
    ? <Message id="quota.windowPhrase.weeklyModel" params={{ model: <InlineReference monospace wrap>{window.label}</InlineReference> }} />
    : undefined;
  return {
    title: <Message id="quota.accountUsedUp.title" params={{ account: <InlineReference dir="auto" wrap>{account}</InlineReference> }} />,
    description: quota
      ? resets ? <Message id="quota.accountUsedUp.descriptionResets" params={{ quota, resets }} /> : <Message id="quota.accountUsedUp.description" params={{ quota }} />
      : plain.description,
  };
}
