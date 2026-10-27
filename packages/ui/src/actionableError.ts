import { DaemonRequestError } from "./daemon-client";
import type { ErrorRecovery } from "./errorRecovery";
import type { MessageParams } from "./protocol";

/** A local failure whose user remedy is known without inspecting its translated message. Its message is the
 * application's own words; `detail` is the raw text it carries (what the system said), which is never part of the
 * message and is shown apart from it, behind Details. An operating-system signature in it or its cause still takes
 * over `recovery` (see `classifyError`), and those name remedies for writing (access, disk space): a failed read
 * passes `fileRead`, which always wins. */
export class ActionableError extends DaemonRequestError {
  readonly detail?: string;
  constructor(message: string, readonly recovery: ErrorRecovery, options?: ErrorOptions & { code?: string; params?: MessageParams; detail?: string }) {
    super(message, options?.code ?? "ui_actionable", options?.params ?? {});
    this.cause = options?.cause;
    this.detail = options?.detail;
  }
}
