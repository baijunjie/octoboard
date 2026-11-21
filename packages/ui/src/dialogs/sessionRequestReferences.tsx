import React from "react";

import { defaultAccountName } from "../agents/agents";
import { InlineReference } from "../i18n/InlineReference";
import { Message } from "../i18n/react";
import type { Translate } from "../i18n/catalog";
import type { Account, Agent, Console, Project, Session, Settings } from "../protocol";
import { sessionDisplayTitle } from "../sessions/sessionLabel";
import type { SessionRequest } from "../store";

/** A thing the user named, as a reference in a sentence: by its name, else, for one the store does not know, by its id
 * as code. */
export function recordReference(name: string | undefined, id: string, key?: number): React.ReactElement {
  return <InlineReference key={key} monospace={name === undefined} dir={name === undefined ? "ltr" : "auto"} wrap>{name ?? id}</InlineReference>;
}

/** The account that holds a conversation's record, as the pickers name it: the default account of
 * `agent` for `null`, a stored account by its name, else by its id, as any other id the store does
 * not know. */
export function recordAccount(t: Translate, accounts: Account[], defaultNames: Settings["default_account_names"], agent: Agent, account: string | null): React.ReactElement {
  const name = account === null ? defaultAccountName(t, defaultNames, agent) : accounts.find((item) => item.id === account)?.name;
  return recordReference(name, account ?? "");
}

/** The session that makes a request, named where it lives: by its display title, its current task beside its name, and
 * its project and console only where they are not the ones the request is for (`request.project`, `request.console`),
 * which the sentence names anyway. Named by its id, with nothing of its place, where the store does not know it. */
export function requesterOf(
  t: Translate,
  request: Pick<SessionRequest, "session" | "console" | "project">,
  requesting: Session | undefined,
  consoles: Map<string, Console>,
  projects: Map<string, Project>,
): React.ReactElement {
  const session = recordReference(requesting ? sessionDisplayTitle(t, requesting) : undefined, request.session);
  if (!requesting || (requesting.project_id && requesting.project_id === request.project)) {
    return <Message id="dialog.sessionRequest.requester.session" params={{ session }} />;
  }
  const elsewhere = requesting.console_id !== request.console;
  const own = { session, console: recordReference(consoles.get(requesting.console_id)?.name, requesting.console_id) };
  if (requesting.project_id) {
    const project = recordReference(projects.get(requesting.project_id)?.name, requesting.project_id);
    return elsewhere
      ? <Message id="dialog.sessionRequest.requester.projectElsewhere" params={{ ...own, project }} />
      : <Message id="dialog.sessionRequest.requester.project" params={{ session, project }} />;
  }
  return elsewhere
    ? <Message id="dialog.sessionRequest.requester.consoleElsewhere" params={own} />
    : <Message id="dialog.sessionRequest.requester.console" params={{ session }} />;
}
