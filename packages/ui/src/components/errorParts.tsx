import React from "react";

import { DaemonRequestError } from "../daemon-client";
import { hasCodeParams, hasShellTimeoutDetails, isStructuredFailure, isUncataloguedFailure, type ErrorPresentation } from "../errorPresentation";
import { errorText } from "../errorText";
import { isolate } from "../i18n/isolate";
import type { MessageParams } from "../protocol";
import { DaemonErrorMessage, type DaemonErrorPart } from "./DaemonErrorMessage";
import { hasErrorDetails, hasErrorRemedy, hasRawDetails } from "./errorPartPresence";

/** The details of a failure for an alert's Details popover (further explanation, then raw text), or
 * nothing when there is neither. */
export function errorDetails(error: ErrorPresentation): React.ReactNode {
  return typeof error !== "string" && hasErrorDetails(error) ? <DaemonErrorMessage error={error} part="details" /> : undefined;
}

/** The raw text of a failure for a disclosure of its own (the error alert dialog's "Error details"), or nothing
 * when it has none. A toast has no Details, so it holds none of this. */
export function rawErrorDetails(error: ErrorPresentation): React.ReactNode {
  return typeof error !== "string" && hasRawDetails(error) ? <DaemonErrorMessage error={error} part="raw" /> : undefined;
}

/** The one main thing to do about a failure, in the application's own words, shown in the alert's
 * description and never behind Details, or nothing when the failure has none. */
export function errorRemedy(error: ErrorPresentation): React.ReactNode {
  return typeof error !== "string" && hasErrorRemedy(error) ? <DaemonErrorMessage error={error} part="remedy" /> : undefined;
}

/** The popover body of a failure mark, which is itself the details surface: the sentence (`summary`, or
 * the failure's own), the main remedy under it, then the details (further explanation, raw text last). */
export function FailureBody({ error, summary }: { error: ErrorPresentation; summary?: React.ReactNode }): React.ReactElement {
  const remedy = errorRemedy(error);
  return (
    <>
      <p>{summary ?? <DaemonErrorMessage error={error} part="summary" />}</p>
      {remedy && <p>{remedy}</p>}
      {errorDetails(error)}
    </>
  );
}

/** `FailureBody` for a failure worded as `text` in the current language (see `codedMessageNode`). */
export function CodedFailureBody({ failure, text }: { failure: { code: string; params: MessageParams }; text: string }): React.ReactElement {
  return <FailureBody error={new DaemonRequestError(text, failure.code, failure.params)} summary={codedMessageNode(failure, text, "summary")} />;
}

/** A failure worded as `text` (in the current language), as a node: where its message sets some params
 * as code they are set as code, anywhere else it is the text itself. A failure the application has no
 * words for is its generic sentence, and `text`, which is then the raw message, goes to the `details`. */
export function codedMessageNode(failure: { code: string; params: MessageParams }, text: string, part?: DaemonErrorPart): React.ReactNode {
  const error = new DaemonRequestError(text, failure.code, failure.params);
  if (part === "details") return errorDetails(error) ?? null;
  if (part === "remedy") return errorRemedy(error) ?? null;
  if (isUncataloguedFailure(failure)) return <DaemonErrorMessage error={error} part={part} />;
  return hasCodeParams(failure) || (part && hasShellTimeoutDetails(failure)) ? <DaemonErrorMessage error={error} part={part} /> : text;
}

/** A failure as a param of a sentence: the structured message where it has one, otherwise its text
 * isolated, as any value put inside a sentence is. */
export function failureParam(error: ErrorPresentation): React.ReactNode {
  return typeof error !== "string" && isStructuredFailure(error) ? <DaemonErrorMessage error={error} showTitle={false} part="summary" /> : isolate(errorText(error));
}
