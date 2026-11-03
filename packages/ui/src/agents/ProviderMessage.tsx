import React from "react";

import { InlineReference } from "../i18n/InlineReference";
import { Message, useT } from "../i18n/react";
import type { MessageKey } from "../i18n/catalog";
import { providerReferences } from "../providers/registry";
import type { ProviderMessageName } from "../providers/types";
import type { ProviderInfo } from "../protocol";
import { providerLabel, providerMessageKey } from "./providers";

/** The provider `id`'s message `name` from its presentation module, with its `{agent}` the provider's name and each of
 * the module's references for it set apart as monospace (`unbroken` keeps one from wrapping inside). Where the module
 * has none, the `fallback` message is shown with `{agent}` alone, or nothing without one. */
export function ProviderMessage({ providers, id, name, fallback, unbroken }: {
  /** The providers the daemon serves, which name a provider that has no presentation module. */
  providers: readonly ProviderInfo[];
  id: string;
  name: ProviderMessageName;
  fallback?: MessageKey;
  unbroken?: boolean;
}): React.ReactElement | null {
  const t = useT();
  const key = providerMessageKey(id, name);
  const shown = key ?? fallback;
  if (!shown) return null;
  const references = key ? Object.entries(providerReferences(id, name)) : [];
  const params = {
    agent: <InlineReference dir="auto" wrap>{providerLabel(t, providers, id)}</InlineReference>,
    ...Object.fromEntries(references.map(([placeholder, text]) => [placeholder, <InlineReference key={placeholder} dir="ltr" monospace wrap unbroken={unbroken}>{text}</InlineReference>])),
  };
  return <Message id={shown} params={params as never} />;
}
