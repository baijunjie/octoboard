import React, { useRef, useState } from "react";

import { DiagnosticBlock } from "../components/alerts/DiagnosticBlock";
import { DetailsDisclosure } from "../components/alerts/ErrorDetailsDisclosure";
import { alertFailure, raiseError } from "../components/alerts/raiseError";
import { ConfirmDialog } from "../components/dialog/ConfirmDialog";
import { settle as settleAction } from "../components/hooks/settle";
import { ActionableError } from "../errors/actionableError";
import { InlineReference } from "../i18n/InlineReference";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { RemoteDaemonConnectOutcome, RemoteDaemonEntry, RemoteDaemonsCapability } from "../platform";
import { RemoteDaemonPathDialog, RemoteDaemonSecretDialog } from "./RemoteDaemonPrompts";

type GenerationMismatch = Extract<RemoteDaemonConnectOutcome, { outcome: "generation_mismatch" }>;

/** What a connection waits on from the owner, between two attempts. */
type Step =
  | { kind: "secret"; secretKind: "password" | "passphrase"; deviceKeyRefused?: boolean }
  | { kind: "host_key"; fingerprint: string }
  | { kind: "no_daemon" }
  | { kind: "application_not_running"; detail: string }
  | { kind: "path"; tried: string[] }
  | { kind: "mismatch"; mismatch: GenerationMismatch };

/** One connection in progress: what the owner has given so far, which the retries of this one connection reuse and
 * which is dropped as the connection ends. */
interface Flow {
  entry: RemoteDaemonEntry;
  secret: string | null;
  confirmFingerprint: string | null;
  step: Step;
}

/** The command that prints a server's host key fingerprint, as the host key dialog tells the owner to run it. */
const FINGERPRINT_COMMAND = "ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub";

/**
 * Connecting to a remote daemon, from the press on Connect to the daemon's window opening: asks for the password first
 * (a key's passphrase only when the shell asks, and nothing for this device's own key unless the server refuses it, when the
 * password signs in again and the shell reinstalls the key), and each outcome that is not the window opening is an
 * alert or a question to answer before a retry (see `RemoteDaemonConnectOutcome`). The password, the passphrase and the
 * fingerprint just confirmed live in the flow's state alone, for the retries of this one connection. A version mismatch
 * offers to open the daemon's own web interface in the system browser (the shell holds the connection for it and opens
 * the browser itself, `opened_in_browser`); it never updates either side, which is done on each machine itself. One
 * entry is connected at a time. `connecting` is the id of the entry being connected, to show its row pending and the
 * others' Connect disabled, and `dialogs` is what the flow has in front, to render beside the list; `onChanged` runs
 * when the shell may have changed the list (it pins the host key itself), and `onAttention` when a connection ended in
 * something to tell or ask the owner, so that the page showing it can be brought up. A connection asked for elsewhere
 * (a menu's saved daemon) is made by the shell, which hands what has to be shown to the Remote devices page as an
 * outcome, which `resume` takes up where `attempt` would have.
 */
export function useRemoteDaemonConnect(capability: RemoteDaemonsCapability, onChanged: () => void, onAttention?: () => void): {
  connecting: string | undefined;
  /** Starts a connection; `held` for an entry the shell already holds a connection to, by a window it brings forward or
   * for the browser it opens again, which so asks for no password. */
  connect: (entry: RemoteDaemonEntry, held?: boolean) => void;
  /** Continues a connection the shell made away from the page, which ended with a question or a failure `outcome`
   * (`onHandedOver`), as if it had been started here; refused while a connection of this page is under way. */
  resume: (entry: RemoteDaemonEntry, outcome: RemoteDaemonConnectOutcome) => void;
  /** Whether no connection is under way and no question is open, so a handed-over outcome can be taken. */
  idle: boolean;
  dialogs: React.ReactElement | null;
} {
  const t = useT();
  const [connecting, setConnecting] = useState<string>();
  const [flow, setFlow] = useState<Flow>();
  // One connection attempt at a time: the rows' Connect is disabled while one runs (`connecting`), and this holds the
  // line in the moment before that renders.
  const running = useRef(false);

  const fail = (title: string, message: string, detail?: string, content?: React.ReactNode) => {
    raiseError(new ActionableError(message, "none", { detail }), { title, keepTitle: true, content });
  };

  /** What the owner is told or asked once a connection has ended with `outcome`, whoever asked for it. */
  const settle = (entry: RemoteDaemonEntry, secret: string | null, confirmFingerprint: string | null, outcome: RemoteDaemonConnectOutcome) => {
    const address = `${entry.host}:${entry.port}`;
    if (outcome.outcome !== "opened" && outcome.outcome !== "opened_in_browser") onAttention?.();
    const ask = (step: Step) => setFlow({ entry, secret, confirmFingerprint, step });
    switch (outcome.outcome) {
      // The daemon's window is open or focused, or the shell has opened the browser: nothing more to do.
      case "opened":
      case "opened_in_browser":
        break;
      case "secret_needed":
        ask({ kind: "secret", secretKind: outcome.kind });
        break;
      case "host_key_unknown":
        ask({ kind: "host_key", fingerprint: outcome.fingerprint });
        break;
      case "host_key_mismatch":
        fail(
          t("remoteDaemons.error.hostKeyMismatch.title"),
          `${t("remoteDaemons.error.hostKeyMismatch.body")}${t("common.sentenceSeparator")}${t("remoteDaemons.error.hostKeyMismatch.remedy")}`,
          t("remoteDaemons.error.hostKeyMismatch.details", { expected: outcome.expected, actual: outcome.actual }),
        );
        break;
      case "unreachable":
        // The address is a chip in the alert's body; the message keeps it as text for the report.
        fail(
          t("remoteDaemons.error.unreachable.title"),
          t("remoteDaemons.error.unreachable.body", { address: isolate(address) }),
          outcome.detail,
          <Message id="remoteDaemons.error.unreachable.body" params={{ address: <InlineReference monospace dir="ltr" wrap>{address}</InlineReference> }} />,
        );
        break;
      case "device_key_refused":
        ask({ kind: "secret", secretKind: "password", deviceKeyRefused: true });
        break;
      case "auth_failed":
        fail(t("remoteDaemons.error.authFailed.title"), t("remoteDaemons.error.authFailed.body"), outcome.detail);
        break;
      case "key_unreadable":
        fail(t("remoteDaemons.error.keyUnreadable.title"), t("remoteDaemons.error.keyUnreadable.body"), outcome.detail);
        break;
      case "octoboardd_not_found":
        ask({ kind: "path", tried: outcome.tried });
        break;
      case "no_daemon":
        ask({ kind: "no_daemon" });
        break;
      case "application_not_running":
        ask({ kind: "application_not_running", detail: outcome.detail });
        break;
      case "no_owner_endpoint":
        fail(t("remoteDaemons.error.noOwnerEndpoint.title"), t("remoteDaemons.error.noOwnerEndpoint.body"), outcome.detail);
        break;
      case "endpoint_unreachable":
        fail(t("remoteDaemons.error.endpointUnreachable.title"), t("remoteDaemons.error.endpointUnreachable.body"), outcome.detail);
        break;
      case "generation_mismatch":
        ask({ kind: "mismatch", mismatch: outcome });
        break;
      case "action_failed":
        fail(t(`remoteDaemons.error.${outcome.action}Failed.title`), t(`remoteDaemons.error.${outcome.action}Failed.body`), outcome.detail);
        break;
      // A failure the app does not recognise is unexpected: the alert says so and offers to report it, with what the
      // shell said behind Error details.
      case "failed":
        raiseError(new Error(outcome.detail || "connection failed"), { title: t("remoteDaemons.error.failed.title") });
        break;
    }
  };

  const attempt = async (entry: RemoteDaemonEntry, secret: string | null, confirmFingerprint: string | null, action: "start" | null, browser = false) => {
    if (running.current) return;
    running.current = true;
    setFlow(undefined);
    setConnecting(entry.id);
    await settleAction(
      async () => settle(entry, secret, confirmFingerprint, await capability.connect({ id: entry.id, secret, confirmFingerprint, action, browser })),
      (error) => raiseError(error, { title: t("remoteDaemons.error.failed.title") }),
      () => {
        running.current = false;
        setConnecting(undefined);
        onChanged();
      },
    );
  };

  const connect = (entry: RemoteDaemonEntry, held = false) => {
    if (running.current) return;
    if (entry.auth === "password" && !held) setFlow({ entry, secret: null, confirmFingerprint: null, step: { kind: "secret", secretKind: "password" } });
    else void attempt(entry, null, null, null);
  };

  const resume = (entry: RemoteDaemonEntry, outcome: RemoteDaemonConnectOutcome) => {
    if (running.current) return;
    settle(entry, null, null, outcome);
  };

  const end = () => setFlow(undefined);
  const name = flow ? <InlineReference dir="auto" wrap>{flow.entry.name}</InlineReference> : null;
  let dialogs: React.ReactElement | null = null;
  if (flow) {
    const { entry, secret, confirmFingerprint, step } = flow;
    const errorTitle = t("remoteDaemons.error.failed.title");
    switch (step.kind) {
      case "secret":
        dialogs = <RemoteDaemonSecretDialog kind={step.secretKind} name={entry.name} user={entry.user} message={step.deviceKeyRefused ? t("remoteDaemons.secret.deviceKeyRefused") : undefined} onSubmit={(value) => void attempt(entry, value, confirmFingerprint, null)} onClose={end} />;
        break;
      case "host_key":
        dialogs = (
          <ConfirmDialog
            title={t("remoteDaemons.hostKey.title")}
            errorTitle={errorTitle}
            subject={<Message id="remoteDaemons.hostKey.subject" params={{ name, fingerprint: <InlineReference monospace dir="ltr" wrap>{step.fingerprint}</InlineReference> }} />}
            message={<p><Message id="remoteDaemons.hostKey.compare" params={{ command: <InlineReference monospace dir="ltr" wrap>{FINGERPRINT_COMMAND}</InlineReference> }} /></p>}
            confirmLabel={t("remoteDaemons.connect")}
            onConfirm={async () => void attempt(entry, secret, step.fingerprint, null)}
            onCancel={end}
          />
        );
        break;
      case "no_daemon":
        dialogs = (
          <ConfirmDialog
            title={t("remoteDaemons.noDaemon.title")}
            errorTitle={errorTitle}
            subject={<Message id="remoteDaemons.noDaemon.subject" params={{ name }} />}
            message={t("remoteDaemons.noDaemon.message")}
            confirmLabel={t("remoteDaemons.noDaemon.confirm")}
            onConfirm={async () => void attempt(entry, secret, confirmFingerprint, "start")}
            onCancel={end}
          />
        );
        break;
      case "application_not_running":
        dialogs = (
          <ConfirmDialog
            title={t("remoteDaemons.applicationNotRunning.title")}
            errorTitle={errorTitle}
            subject={<Message id="remoteDaemons.applicationNotRunning.subject" params={{ name }} />}
            message={
              <>
                <p><Message id="remoteDaemons.applicationNotRunning.message" /></p>
                {step.detail !== "" && (
                  <DetailsDisclosure>
                    <DiagnosticBlock>{step.detail}</DiagnosticBlock>
                  </DetailsDisclosure>
                )}
              </>
            }
            confirmLabel={t("error.tryAgain")}
            onConfirm={async () => void attempt(entry, secret, confirmFingerprint, null)}
            onCancel={end}
          />
        );
        break;
      case "path":
        dialogs = (
          <RemoteDaemonPathDialog
            tried={step.tried}
            onSave={async (path) => {
              const saved = await capability.save({ ...entry, octoboarddPath: path });
              void attempt(saved, secret, confirmFingerprint, null);
            }}
            onClose={end}
          />
        );
        break;
      case "mismatch": {
        // Nothing here updates either side: updating is done on each machine itself. The dialog says which side is
        // behind and offers the daemon's own web interface, which matches its version; with this app behind, it also
        // offers Settings' Updates in this app's own window, the × then being the way out.
        const older = step.mismatch.remoteGeneration < step.mismatch.localGeneration;
        dialogs = (
          <ConfirmDialog
            title={t("remoteDaemons.mismatch.title")}
            errorTitle={errorTitle}
            hideCancel={!older}
            otherAction={older ? undefined : {
              label: t("remoteDaemons.mismatch.goToUpdates"),
              onPress: () => {
                end();
                capability.showUpdates().catch(alertFailure({ title: t("remoteDaemons.mismatch.goToUpdatesFailedTitle") }));
              },
            }}
            subject={<Message id="remoteDaemons.mismatch.subject" params={{ name, remoteVersion: <InlineReference monospace>{step.mismatch.remoteVersion}</InlineReference>, localVersion: <InlineReference monospace>{step.mismatch.localVersion}</InlineReference> }} />}
            message={`${t(older ? "remoteDaemons.mismatch.older" : "remoteDaemons.mismatch.newer")}${t("common.sentenceSeparator")}${t("remoteDaemons.mismatch.browser")}`}
            confirmLabel={t("remoteDaemons.browser.open")}
            onConfirm={async () => void attempt(entry, secret, confirmFingerprint, null, true)}
            onCancel={end}
          />
        );
        break;
      }
    }
  }
  return { connecting, connect, resume, idle: connecting === undefined && flow === undefined, dialogs };
}
