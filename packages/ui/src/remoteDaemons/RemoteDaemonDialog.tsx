import React, { useId, useState } from "react";

import { Dialog } from "../components/dialog/Dialog";
import { useDialogAction, useSubmitValidation } from "../components/dialog/dialogHooks";
import { TextInput } from "../components/form/TextInput";
import { InlineReference } from "../i18n/InlineReference";
import { Message, useT } from "../i18n/react";
import type { RemoteDaemonAuth, RemoteDaemonEntry, RemoteDaemonEntryInput } from "../platform";
import { Button, Radio, RadioGroup } from "../primitives";
import { parseConnectionDetails } from "./connectionDetails";
import { DEFAULT_SSH_PORT, parsePort } from "./port";

/** The examples the fields show, formats that are the same in every language. */
const EXAMPLES = {
  details: "octoboard+ssh://alex@server.example.com:22",
  host: "server.example.com",
  port: String(DEFAULT_SSH_PORT),
  user: "alex",
  keyPath: "~/.ssh/id_ed25519",
} as const;

/** The command that prints a server's connection details, the same in every language. */
const CONNECTION_COMMAND = "octoboardd connection";

/**
 * The form that adds a remote daemon or edits one (`entry`). A blank name is saved as the host. Connection details
 * `octoboardd connection` printed on the server, pasted into the first field, fill the host, port, user, the path of
 * `octoboardd`, the name where they carry one, and the host key to pin. The pinned host key is shown and can be
 * cleared, so that a server whose key has really changed can be confirmed again on the next connection; it is never
 * typed in. This device's own key is offered only to an entry the shell has already switched to it, since the shell
 * installs it at a sign-in with one of the others. `onSave` sends the entry, and the dialog closes when it resolves and
 * shows its rejection otherwise.
 */
export function RemoteDaemonDialog({ entry, onSave, onClose }: {
  entry?: RemoteDaemonEntry;
  onSave: (entry: RemoteDaemonEntryInput) => Promise<void>;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const fingerprintLabelId = useId();
  const [name, setName] = useState(entry?.name ?? "");
  const [host, setHost] = useState(entry?.host ?? "");
  const [port, setPort] = useState(String(entry?.port ?? DEFAULT_SSH_PORT));
  const [user, setUser] = useState(entry?.user ?? "");
  const [auth, setAuth] = useState<RemoteDaemonAuth>(entry?.auth ?? "password");
  const [keyPath, setKeyPath] = useState(entry?.keyPath ?? "");
  const [octoboarddPath, setOctoboarddPath] = useState(entry?.octoboarddPath ?? "");
  const [fingerprint, setFingerprint] = useState(entry?.fingerprint ?? null);
  const [details, setDetails] = useState("");
  const { busy, run } = useDialogAction(undefined, t("remoteDaemons.form.saveFailedTitle"));
  const { shown, attempt } = useSubmitValidation();
  const portNumber = parsePort(port);
  const hostError = host ? undefined : t("remoteDaemons.form.host.required");
  const portError = portNumber === undefined ? t("remoteDaemons.form.port.invalid") : undefined;
  const userError = user ? undefined : t("remoteDaemons.form.user.required");
  const keyPathError = auth === "key" && !keyPath ? t("remoteDaemons.form.keyPath.required") : undefined;
  const detailsInvalid = !!details.trim() && !parseConnectionDetails(details);
  const detailsError = detailsInvalid ? t("remoteDaemons.form.details.invalid", { command: CONNECTION_COMMAND }) : undefined;
  const example = (value: string) => t("common.example", { example: value });

  const changeDetails = (text: string) => {
    setDetails(text);
    const parsed = parseConnectionDetails(text);
    if (!parsed) return;
    setHost(parsed.host);
    setPort(String(parsed.port));
    setUser(parsed.user);
    setOctoboarddPath(parsed.octoboarddPath ?? "");
    // Details that carry no fingerprint leave a pinned one in place: a pin is removed only by Clear.
    if (parsed.fingerprint) setFingerprint(parsed.fingerprint);
    if (parsed.name) setName(parsed.name);
  };

  const submit = () => {
    if (!attempt(detailsError, hostError, portError, userError, keyPathError) || portNumber === undefined) return;
    void run(async () => {
      await onSave({
        id: entry?.id ?? null,
        name: name || host,
        host,
        port: portNumber,
        user,
        auth,
        keyPath: auth === "key" ? keyPath : null,
        fingerprint,
        octoboarddPath: octoboarddPath || null,
      });
      onClose();
    });
  };

  return (
    <Dialog title={t(entry ? "remoteDaemons.form.editTitle" : "remoteDaemons.add")} onClose={onClose} submitLabel={t("common.save")} busy={busy} onSubmit={submit}>
      <TextInput
        label={t("remoteDaemons.form.details")}
        value={details}
        onChange={changeDetails}
        placeholder={example(EXAMPLES.details)}
        description={<Message id="remoteDaemons.form.details.description" params={{ command: <InlineReference monospace>{CONNECTION_COMMAND}</InlineReference> }} />}
        dir="ltr"
        errorMessage={shown(detailsError) && <Message id="remoteDaemons.form.details.invalid" params={{ command: <InlineReference monospace>{CONNECTION_COMMAND}</InlineReference> }} />}
        autoFocus={!entry}
      />
      <TextInput label={t("remoteDaemons.form.name")} value={name} onChange={setName} placeholder={t("remoteDaemons.form.name.placeholder")} autoFocus={!!entry} />
      <TextInput label={t("remoteDaemons.form.host")} value={host} onChange={setHost} placeholder={example(EXAMPLES.host)} dir="ltr" errorMessage={shown(hostError)} />
      <TextInput label={t("remoteDaemons.form.port")} value={port} onChange={setPort} placeholder={example(EXAMPLES.port)} dir="ltr" errorMessage={shown(portError)} />
      <TextInput label={t("remoteDaemons.form.user")} value={user} onChange={setUser} placeholder={example(EXAMPLES.user)} dir="ltr" errorMessage={shown(userError)} />
      <RadioGroup variant="secondary" label={t("remoteDaemons.form.auth")} value={auth} onChange={(value) => setAuth(value as RemoteDaemonAuth)}>
        <Radio value="password">{t("remoteDaemons.form.auth.password")}</Radio>
        <Radio value="key">{t("remoteDaemons.form.auth.key")}</Radio>
        {entry?.auth === "device" && (
          <Radio value="device" description={t("remoteDaemons.form.auth.device.description")}>{t("remoteDaemons.form.auth.device")}</Radio>
        )}
      </RadioGroup>
      {auth === "key" && (
        <TextInput
          label={t("remoteDaemons.form.keyPath")}
          value={keyPath}
          onChange={setKeyPath}
          placeholder={example(EXAMPLES.keyPath)}
          description={<Message id="remoteDaemons.form.keyPath.description" params={{ tilde: <InlineReference monospace>~</InlineReference> }} />}
          dir="ltr"
          errorMessage={shown(keyPathError)}
        />
      )}
      <TextInput
        label={t("remoteDaemons.form.octoboardd")}
        value={octoboarddPath}
        onChange={setOctoboarddPath}
        placeholder={t("remoteDaemons.form.octoboardd.placeholder")}
        description={<Message id="remoteDaemons.form.octoboardd.description" params={{ octoboardd: <InlineReference monospace>octoboardd</InlineReference> }} />}
        dir="ltr"
      />
      {(entry || fingerprint !== null) && (
        <div role="group" aria-labelledby={fingerprintLabelId} className="flex flex-col gap-2 text-sm">
          <span id={fingerprintLabelId} className="font-medium">{t("remoteDaemons.form.fingerprint")}</span>
          {fingerprint === null ? (
            <p className="text-app-muted">{t("remoteDaemons.form.fingerprint.none")}</p>
          ) : (
            <>
              <div className="flex items-center justify-between gap-3">
                <span dir="ltr" className="min-w-0 font-mono [overflow-wrap:anywhere]">{fingerprint}</span>
                <Button type="button" size="sm" variant="secondary" onPress={() => setFingerprint(null)}>{t("common.clear")}</Button>
              </div>
              <p className="text-app-muted">{t("remoteDaemons.form.fingerprint.description")}</p>
            </>
          )}
        </div>
      )}
    </Dialog>
  );
}
