import { AlertDialog } from "@heroui/react";
import React, { useLayoutEffect, useRef, useState, useSyncExternalStore } from "react";

import { Button } from "../components/Button";
import { recoveryAction } from "../components/ErrorGuidance";
import { alertFailures, errorAlerts } from "../errorAlerts";
import { useT } from "../i18n/react";
import { openSettingsFromAnywhere } from "../settings/settingsLauncher";
import { DaemonProvider, type Daemon } from "../store";
import { useIssueReport } from "../useIssueReport";
import { dialogAround } from "./dialogElement";
import { Dialog } from "./Dialog";
import { openSessionModelFromAnywhere } from "./sessionModelLauncher";
import { ErrorAlertBody, alertTitle } from "./ErrorAlertBody";

/** Mounted outside the application boundary, so an alert raised while the application is not on screen can still be
 * acknowledged. */
export function ErrorAlerts({ daemon }: { daemon?: Daemon }): React.ReactElement {
  const host = <ErrorAlertHost />;
  // The report reads the daemon's records, so it is inside the provider too.
  return daemon ? <DaemonProvider value={daemon}>{host}</DaemonProvider> : host;
}

function ErrorAlertHost(): React.ReactElement | null {
  const t = useT();
  const [raised] = useSyncExternalStore(errorAlerts.subscribe, errorAlerts.getSnapshot);
  // Show an alert only from the commit after the one it was raised in. A dialog opened in the same commit (a press
  // that opens it and reports a failure in one task) hides the rest of the page with react-aria's `ariaHideOutside`
  // from a passive effect of that commit: with the alert already in the document, that dialog's would hide the alert
  // too, and the alert's own, which only hides what lies outside it, would never show it again, so the alert stays
  // inert and the dialog, hidden in turn, drops focus to `<body>`. React runs a commit's passive effects before it
  // renders again, so the dialog's has run by the time the alert mounts. The alert is set from a layout effect, which
  // makes that render a synchronous one in the same task, before any frame and before the dialog's hiding answers the
  // alert's insertion, and in time for the focusing below. A dialog that the press's dialog opens in turn from an effect
  // as it mounts still mounts in the alert's commit, so the alert is marked `topLayerAtMount` for that dialog's hiding to
  // leave it out.
  const [error, setError] = useState<typeof raised>();
  useLayoutEffect(() => { setError(raised); }, [raised]);
  const acknowledge = useRef<HTMLButtonElement>(null);
  const issue = useIssueReport(error?.classification.kind === "unexpected" ? alertFailures(error, "report") : undefined);
  // Focus the alert in the render after the one that mounted it, before the task that raised it ends, and nowhere
  // else: its `Dialog` never puts focus on it by itself, not even when the alert it replaces leaves focus on `<body>`
  // (until then it holds focus on an empty element inside the alert; see `focusesItselfOnOpen`).
  // react-aria turns a new modal's focus containment on from a layout effect at mount, which forces a render; the state
  // set here from a layout effect of the same commit is set after it and lands in that render or a later one, and a
  // layout effect of that render runs after the containment's own, which sit below it. Focus must land after that:
  // a dialog under the alert that has no focused element of its own yet (one opened by a press without a pointer,
  // whose own opening focus react-aria's `focusSafely` defers to the next frame or past the open transition) treats
  // focus on the not-yet-containing alert as escaping its containment and queues `focusSafely` on the alert's first
  // control, where Enter would then press it (for Manage accounts…, opening Settings). It must also land before the
  // next frame, because that dialog's deferred opening focus fires then unless focus has moved: once it lands on the
  // dialog, the alert is outside the containment it joins and focus is taken back from it.
  const [mountedId, setMountedId] = useState<number>();
  useLayoutEffect(() => { setMountedId(error?.id); }, [error?.id]);
  useLayoutEffect(() => {
    if (mountedId !== undefined) dialogAround(acknowledge)?.focus();
  }, [mountedId]);
  if (!error) return null;
  const unexpected = error.classification.kind === "unexpected";
  // A button for the remedy when it is the one remedy of every failure the alert is about.
  // Change model needs the session the alert is about; without one there is nothing to open it for.
  const found = error.sharedRecovery ? recoveryAction(t, error.sharedRecovery) : undefined;
  const action = found?.sessionModel && !error.session ? undefined : found;
  return (
    <Dialog
      key={error.id}
      alert
      dismissable={false}
      headerLeading={<AlertDialog.Icon status="danger" />}
      title={alertTitle(t, error)}
      onClose={() => {}}
      focusesItselfOnOpen
      topLayerAtMount
      footer={<>
        {unexpected && issue.link}
        {action && (
          <Button variant="secondary" onPress={() => {
            errorAlerts.acknowledge();
            if (action.sessionModel && error.session) openSessionModelFromAnywhere(error.session);
            else openSettingsFromAnywhere("accounts", { addAgent: action.addAgent });
          }}>{action.label}</Button>
        )}
        <Button ref={acknowledge} variant="danger" onPress={() => errorAlerts.acknowledge()}>{t("error.acknowledge")}</Button>
      </>}
    >
      <ErrorAlertBody alert={error} issueGuidance={issue.guidance} />
    </Dialog>
  );
}
