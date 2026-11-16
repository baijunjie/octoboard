import React from "react";

import { InlineReference } from "../i18n/InlineReference";
import { Message } from "../i18n/react";

/** The account field's error for a chosen account that is turned off or not logged in, the account's name a chip and,
 * for a turned-off one, the Settings where it is turned on again. */
export function accountRefusalMessage(id: "accountChoice.disabled.description" | "accountChoice.signedOut.description", name: string): React.ReactElement {
  const account = <InlineReference dir="auto" wrap>{name}</InlineReference>;
  if (id === "accountChoice.signedOut.description") return <Message id={id} params={{ name: account }} />;
  return <Message id={id} params={{ name: account, settingsDialog: <InlineReference dir="auto" wrap><Message id="settings.title" /></InlineReference> }} />;
}
