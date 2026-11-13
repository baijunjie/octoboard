import React from "react";

import { InlineReference } from "../i18n/InlineReference";
import { Message, useT } from "../i18n/react";
import { StartupFailure, StartupScreen } from "../StartupScreen";

/**
 * The whole window for a page the daemon itself served (the server form's browser build) whose address does not open
 * the daemon: connection state `keyRefused`, where the daemon's key changed under the open page, or `keyless`, an
 * address opened with no key at all. Nothing can be loaded and nothing is retried, and reloading the page only meets the
 * same refusal, so there is no button: the way out is the address `octoboardd address` prints on the server. Report
 * issue follows the instructions as for any failure the application names, in case they do not resolve it.
 */
export function KeyRefusedScreen({ keyless = false }: { keyless?: boolean }): React.ReactElement {
  const t = useT();
  const command = <InlineReference monospace wrap unbroken>octoboardd address</InlineReference>;
  // The report describes the failure in fixed English, never the screen's localized wording.
  const diagnostic = keyless
    ? "The page was opened from the daemon's address with no key."
    : "The daemon refused the key this page was opened with.";
  return (
    <StartupScreen
      message={
        <StartupFailure
          failure={{
            title: t(keyless ? "connection.keyRefused.titleNoKey" : "connection.keyRefused.title"),
            description: t(keyless ? "connection.keyRefused.descriptionNoKey" : "connection.keyRefused.description"),
            remedy: <Message id="connection.keyRefused.remedy" params={{ command }} />,
            details: null,
          }}
          diagnostic={diagnostic}
          known={{ title: keyless ? "This address does not open the daemon" : "This address no longer opens the daemon" }}
        />
      }
    />
  );
}
