import React from "react";

import { providerLabel } from "../../agents/providers";
import { InlineReference } from "../../i18n/InlineReference";
import { useT } from "../../i18n/react";
import { useProviders } from "../../store";

/** The name of provider `agent`, set apart as a reference in the sentence that names it: its presentation module's, else
 * the label the daemon serves of it. */
export function ProviderName({ agent }: { agent: string }): React.ReactElement {
  const t = useT();
  const providers = useProviders();
  return <InlineReference dir="auto" wrap>{providerLabel(t, providers, agent)}</InlineReference>;
}
