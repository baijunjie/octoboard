import React, { useId, useLayoutEffect, useRef, useState, useSyncExternalStore } from "react";

import { RecoveryLink, recoveryAction, type RecoveryAction } from "../components/alerts/ErrorGuidance";
import { alertFailures, errorAlerts } from "../components/alerts/errorAlerts";
import { useT } from "../i18n/react";
import { openSettingsFromAnywhere } from "../settings/settingsLauncher";
import { isKnownSessionStatus } from "../sessions/sessionStatus";
import { DaemonProvider, useDaemonStoreOrEmpty, type Daemon } from "../store";
import { useIssueReport } from "../components/alerts/useIssueReport";
import { Dialog } from "../components/dialog/Dialog";
import { dialogAround } from "../components/dialog/dialogElement";
import { useFocusWhenShown } from "../components/dialog/dialogHooks";
import { openSessionModelFromAnywhere } from "../dialogs/sessionModelLauncher";
import { ErrorAlertBody, alertTitle } from "../components/dialog/ErrorAlertBody";
import { Button, ModalStatusIcon } from "../primitives";

/** The alert's control for its remedy (see `recoveryAction`), a link out, or a button that
 * acknowledges the alert and opens what the remedy is done in. */
function RecoveryControl({ action, session }: { action: RecoveryAction; session: string | undefined }): React.ReactElement {
  switch (action.kind) {
    case "link": return <RecoveryLink action={action} />;
    case "sessionModel":
    case "settings": return (
      <Button variant="secondary" onPress={() => {
        errorAlerts.acknowledge();
        if (action.kind === "settings") openSettingsFromAnywhere("accounts", { addAgent: action.addAgent });
        else if (session) openSessionModelFromAnywhere(session);
      }}>{action.label}</Button>
    );
  }
}

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
  // A marker inside the alert, to find its element from: the footer may have no acknowledgement button to ask.
  const inside = useRef<HTMLSpanElement>(null);
  // Whether the alert's session is one whose status this client does not know (a newer daemon's): nothing that depends
  // on the status is offered for it, a model change included.
  const unknownStatus = useDaemonStoreOrEmpty((s) => {
    const status = error?.session ? s.sessions.get(error.session)?.status : undefined;
    return status !== undefined && !isKnownSessionStatus(status);
  });
  const issue = useIssueReport(error?.classification.kind === "unexpected" ? alertFailures(error, "report") : undefined);
  // What describes an alert that holds a block of its own apart (`attached`): its explanation, not the block.
  const descriptionId = useId();
  // Focus the alert itself each time one is shown, and nowhere else: its `Dialog` never puts focus on it by itself, not
  // even when the alert it replaces leaves focus on `<body>` (until then it holds focus on an empty element inside the
  // alert; see `focusesItselfOnOpen`). Landing on its first control instead, its close button, Enter would acknowledge
  // it unread.
  useFocusWhenShown(() => dialogAround(inside), error !== undefined, error?.id);
  if (!error) return null;
  const unexpected = error.classification.kind === "unexpected";
  // A button for the remedy when it is the one remedy of every failure the alert is about.
  // Change model needs the session the alert is about, and one whose status is known; without that there is nothing to
  // open it for.
  const found = error.sharedRecovery ? recoveryAction(t, error.sharedRecovery) : undefined;
  const session = error.session;
  const action = found?.kind === "sessionModel" && (!session || unknownStatus) ? undefined : found;
  const own = error.action;
  // The close button acknowledges as OK does, on every alert. OK is left out only where the footer would hold three
  // buttons (Report issue, the alert's own action and OK): the close button then stands for it, and the action takes
  // OK's place as the footer's primary, in the accent colour rather than OK's danger one, since it is the way forward,
  // not a destructive step. A remedy a recovery category names never meets Report issue: it comes only with a failure
  // that is not unexpected.
  const acknowledgement = !(own && unexpected && issue.link);
  return (
    <Dialog
      key={error.id}
      alert
      keyboardDismissHeld
      headerLeading={<ModalStatusIcon status="danger" />}
      title={alertTitle(t, error)}
      onClose={() => errorAlerts.acknowledge()}
      focusesItselfOnOpen
      topLayerAtMount
      describedBy={error.attached ? descriptionId : undefined}
      footer={<>
        {unexpected && issue.link}
        {action && <RecoveryControl action={action} session={session} />}
        {own && <Button variant={acknowledgement ? "secondary" : "primary"} onPress={() => { errorAlerts.acknowledge(); own.onPress(); }}>{own.label}</Button>}
        {acknowledgement && <Button variant="danger" onPress={() => errorAlerts.acknowledge()}>{t("error.acknowledge")}</Button>}
      </>}
    >
      <span ref={inside} hidden />
      <ErrorAlertBody alert={error} issueGuidance={issue.guidance} descriptionId={error.attached ? descriptionId : undefined} />
    </Dialog>
  );
}
