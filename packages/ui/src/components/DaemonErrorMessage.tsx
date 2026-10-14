import React from "react";

import { hasShellTimeoutDetails, type ErrorPresentation } from "../errorPresentation";
import { Message, useT } from "../i18n/react";
import { InlineCode } from "./InlineCode";

export function DaemonErrorTitle(): React.ReactElement {
  const t = useT();
  return <>{t("daemon.shell_environment_timeout.title")}</>;
}

/** Structured daemon failures retain their code and parameters; other errors stay plain text. */
export function DaemonErrorMessage({ error, showTitle = true }: { error: ErrorPresentation; showTitle?: boolean }): React.ReactElement {
  const t = useT();
  if (typeof error === "string" || !hasShellTimeoutDetails(error)) return <>{typeof error === "string" ? error : error.message}</>;
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
