import React from "react";

import { ProviderMessage } from "../../agents/ProviderMessage";
import { display } from "../../format/hostPath";
import { InlineReference } from "../../i18n/InlineReference";
import type { ProviderInfo } from "../../protocol";
import { useHostPathsOrEmpty } from "../hooks/useHostPaths";

/** How to log an account of provider `agent` in where the window cannot (`offersLoginFlow`): the provider's own
 * `signInHint`, which names the command of the agent's own CLI that signs an account in, or a sentence that names no
 * agent and no command where its module has none. Either may name the account's configuration directory `dir` (an
 * absolute path on the daemon's host, shown as the window shows such paths) as `{dir}`, the directory the CLI has to be
 * pointed at to sign in that account and not the default one. A caller shows the hint only for a `dir` it knows, never
 * an empty one. `providers` is as `ProviderMessage`'s. */
export function SignInHint({ providers, agent, dir }: { providers: readonly ProviderInfo[]; agent: string; dir: string }): React.ReactElement | null {
  const shown = display(dir, useHostPathsOrEmpty());
  return (
    <ProviderMessage
      providers={providers}
      id={agent}
      name="signInHint"
      fallback="settings.accounts.signInHint"
      params={{ dir: <InlineReference dir="ltr" monospace wrap>{shown}</InlineReference> }}
    />
  );
}
