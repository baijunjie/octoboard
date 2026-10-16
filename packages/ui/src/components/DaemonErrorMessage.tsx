import React from "react";

import { DaemonRequestError } from "../daemon-client";
import { daemonParams, messageCode } from "../daemonMessage";
import { hasCodeParams, hasShellTimeoutDetails, type ErrorPresentation } from "../errorPresentation";
import { errorText } from "../errorText";
import { isMessageKey } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { MessageParams } from "../protocol";
import { useDaemonStore } from "../store";
import { InlineCode } from "./InlineCode";

export function DaemonErrorTitle(): React.ReactElement {
  const t = useT();
  return <>{t("daemon.shell_environment_timeout.title")}</>;
}

const NO_RECORDS = new Map<string, never>();

/** A failure whose message shows some params as code (`CODE_PARAMS`): those set as `InlineCode`, the
 * rest as plain text, in the catalog's wording for the code. */
function CodedDaemonMessage({ error }: { error: DaemonRequestError }): React.ReactElement {
  const language = useCurrentLanguage();
  // The records are subscribed to only for a message that names one, so a failure that does not
  // re-renders with none of them.
  const consoles = useDaemonStore((s) => ("console" in error.params ? s.consoles : NO_RECORDS));
  const projects = useDaemonStore((s) => ("project" in error.params ? s.projects : NO_RECORDS));
  const sessions = useDaemonStore((s) => ("session" in error.params ? s.sessions : NO_RECORDS));
  const id = `daemon.${messageCode(error.code, error.params)}`;
  if (!isMessageKey(id)) return <>{error.message}</>;
  const params = daemonParams(language, error.code, error.params, { consoles, projects, sessions }, (value) => <InlineCode>{value}</InlineCode>);
  // One element, so a container that lays its children out as flex items (an alert's column) keeps
  // the sentence whole instead of making each run of text and each literal an item of its own.
  return <span><Message id={id} params={params as never} /></span>;
}

/** A failure worded as `text` (in the current language), as a node: where its message sets some params
 * as code they are set as code, anywhere else it is the text itself. */
export function codedMessageNode(failure: { code: string; params: MessageParams }, text: string): React.ReactNode {
  return hasCodeParams(failure) ? <DaemonErrorMessage error={new DaemonRequestError(text, failure.code, failure.params)} /> : text;
}

/** A failure as a param of a sentence: the structured message where it has one, otherwise its text
 * isolated, as any value put inside a sentence is. */
export function failureParam(error: ErrorPresentation): React.ReactNode {
  return typeof error !== "string" && hasCodeParams(error) ? <DaemonErrorMessage error={error} showTitle={false} /> : isolate(errorText(error));
}

/** Structured daemon failures retain their code and parameters; other errors stay plain text. */
export function DaemonErrorMessage({ error, showTitle = true }: { error: ErrorPresentation; showTitle?: boolean }): React.ReactElement {
  const t = useT();
  if (typeof error !== "string" && hasCodeParams(error)) return <CodedDaemonMessage error={error} />;
  if (typeof error === "string" || !hasShellTimeoutDetails(error)) return <>{errorText(error)}</>;
  const { shell, command, timeout } = error.params;
  return (
    <div className="flex min-w-0 flex-col gap-2 [overflow-wrap:anywhere]">
      {showTitle && <h3 className="text-base font-semibold text-foreground"><DaemonErrorTitle /></h3>}
      <p>
        <Message
          id="daemon.shell_environment_timeout.command"
          params={{ command: <InlineCode>{`${shell} -l -i -c '${command}'`}</InlineCode>, timeout }}
        />
      </p>
      <p>{t("daemon.shell_environment_timeout.cause")}</p>
      <p>{t("daemon.shell_environment_timeout.consequence")}</p>
      <p>{t("daemon.shell_environment_timeout.recovery")}</p>
    </div>
  );
}
