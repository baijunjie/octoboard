import React from "react";

import { InlineReference } from "../components/InlineReference";
import { Message } from "../i18n/react";

/** The account field's error for a chosen account that is turned off or not logged in, the account's name a chip. */
export function accountRefusalMessage(id: "accountChoice.disabled.description" | "accountChoice.signedOut.description", name: string): React.ReactElement {
  return <Message id={id} params={{ name: <InlineReference dir="auto" wrap>{name}</InlineReference> }} />;
}
