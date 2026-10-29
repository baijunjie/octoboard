import React from "react";

import { FadeOverflow } from "../components/FadeOverflow";
import type { QuotaReading } from "../quota/useAccountQuota";

/** The email an account is signed in with, as its reading gives it; undefined while there is none. */
function accountEmailOf(quota: QuotaReading): string | undefined {
  return typeof quota === "object" && quota.email ? quota.email : undefined;
}

/**
 * The email an account is signed in with, muted, under its name; nothing when the reading has none. By default it
 * wraps like the name does; `clip` keeps it to one line that fades at the end, for a narrow cell, with the whole
 * address in a tooltip while it is clipped.
 */
export function AccountEmail({ quota, clip = false }: { quota: QuotaReading; clip?: boolean }): React.ReactElement | null {
  const email = accountEmailOf(quota);
  if (!email) return null;
  if (clip) return <FadeOverflow dir="ltr" titleWhenClipped={email} className="text-xs font-normal text-app-muted">{email}</FadeOverflow>;
  return <span dir="ltr" className="min-w-0 align-match-parent text-app-muted wrap-anywhere">{email}</span>;
}
