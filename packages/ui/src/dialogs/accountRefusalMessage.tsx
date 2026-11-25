import React from "react";

import { offersLoginFlow } from "../agents/providers";
import { SignInHint } from "../components/text/SignInHint";
import type { Agent, ProviderInfo } from "../protocol";
import { InlineReference } from "../i18n/InlineReference";
import { Message } from "../i18n/react";

/** What names how to log the refused account in, where its provider has no login flow: the providers the daemon serves,
 * the account's agent and its configuration directory (`accountConfigDir`), empty where it is not known. */
export interface SignInContext {
  providers: readonly ProviderInfo[];
  agent: Agent;
  dir: string;
}

/** The account field's error for a chosen account that is turned off or not logged in, the account's name a chip and,
 * for a turned-off one, the Settings where it is turned on again. For one not logged in whose provider has no login
 * flow (`offersLoginFlow`), the way to log in is the provider's own (`SignInHint`) in place of the advice to log in,
 * which is why the `signIn` context is required for that message. A directory that is not known (`dir` empty) has no hint to say, so the
 * advice to log in stays. */
export function accountRefusalMessage(id: "accountChoice.disabled.description", name: string): React.ReactElement;
export function accountRefusalMessage(id: "accountChoice.signedOut.description", name: string, signIn: SignInContext): React.ReactElement;
export function accountRefusalMessage(id: "accountChoice.disabled.description" | "accountChoice.signedOut.description", name: string, signIn?: SignInContext): React.ReactElement {
  const account = <InlineReference dir="auto" wrap>{name}</InlineReference>;
  if (id === "accountChoice.signedOut.description") {
    if (signIn && signIn.dir !== "" && !offersLoginFlow(signIn.providers, signIn.agent)) return <Message id="accountChoice.signedOut.descriptionHint" params={{ name: account, hint: <SignInHint {...signIn} /> }} />;
    return <Message id={id} params={{ name: account }} />;
  }
  return <Message id={id} params={{ name: account, settingsDialog: <InlineReference dir="auto" wrap><Message id="settings.title" /></InlineReference> }} />;
}
