import React, { useState } from "react";

import { Dialog } from "../components/dialog/Dialog";
import { useDialogAction, useSubmitValidation } from "../components/dialog/dialogHooks";
import { TextInput } from "../components/form/TextInput";
import { InlineReference } from "../i18n/InlineReference";
import { Message, useT } from "../i18n/react";

/** Asks for the password of a server, or the passphrase of a key file, for one connection to the remote daemon `name`
 * as `user`: it is not stored, and the dialog only hands it to `onSubmit`. `message` says why it is asked for, where
 * that is not plain. */
export function RemoteDaemonSecretDialog({ kind, name, user, message, onSubmit, onClose }: {
  kind: "password" | "passphrase";
  name: string;
  user: string;
  message?: string;
  onSubmit: (secret: string) => void;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const [secret, setSecret] = useState("");
  const { shown, attempt } = useSubmitValidation();
  const error = secret ? undefined : t(kind === "password" ? "remoteDaemons.secret.password.required" : "remoteDaemons.secret.passphrase.required");
  return (
    <Dialog
      title={t(kind === "password" ? "remoteDaemons.secret.password.title" : "remoteDaemons.secret.passphrase.title")}
      onClose={onClose}
      submitLabel={t("remoteDaemons.connect")}
      onSubmit={() => attempt(error) && onSubmit(secret)}
      size="sm"
    >
      <p className="text-sm"><Message id="remoteDaemons.secret.subject" params={{ name: <InlineReference dir="auto" wrap>{name}</InlineReference>, user: <InlineReference monospace>{user}</InlineReference> }} /></p>
      {message && <p className="text-sm">{message}</p>}
      <TextInput
        type="password"
        label={t(kind === "password" ? "remoteDaemons.secret.password.label" : "remoteDaemons.secret.passphrase.label")}
        value={secret}
        onChange={setSecret}
        placeholder={t(kind === "password" ? "remoteDaemons.secret.password.placeholder" : "remoteDaemons.secret.passphrase.placeholder")}
        description={t("remoteDaemons.secret.description")}
        errorMessage={shown(error)}
        autoFocus
      />
    </Dialog>
  );
}

/** Where `octoboardd` might be on a server, as the path field's example: the same in every language. */
const OCTOBOARDD_EXAMPLE = "/usr/local/bin/octoboardd";

/** Asks where `octoboardd` is on the server, which the shell did not find in `tried`. `onSave` stores the path and
 * connects again; the dialog closes when it resolves and shows its rejection otherwise. */
export function RemoteDaemonPathDialog({ tried, onSave, onClose }: {
  tried: string[];
  onSave: (path: string) => Promise<void>;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const [path, setPath] = useState("");
  const { busy, run } = useDialogAction(undefined, t("remoteDaemons.form.saveFailedTitle"));
  const { shown, attempt } = useSubmitValidation();
  const error = path ? undefined : t("remoteDaemons.notFound.required");
  return (
    <Dialog
      title={t("remoteDaemons.notFound.title")}
      onClose={onClose}
      submitLabel={t("remoteDaemons.notFound.save")}
      busy={busy}
      onSubmit={() => {
        if (!attempt(error)) return;
        void run(async () => {
          await onSave(path);
          onClose();
        });
      }}
    >
      <p className="text-sm"><Message id="remoteDaemons.notFound.description" params={{ octoboardd: <InlineReference monospace>octoboardd</InlineReference> }} /></p>
      {tried.length > 0 && (
        <div className="flex flex-col gap-1 text-sm">
          <span className="font-medium">{t("remoteDaemons.notFound.tried")}</span>
          <ul dir="ltr" className="flex flex-col items-start gap-1 font-mono [overflow-wrap:anywhere]">
            {tried.map((place) => <li key={place}>{place}</li>)}
          </ul>
        </div>
      )}
      <TextInput
        label={t("remoteDaemons.form.octoboardd")}
        value={path}
        onChange={setPath}
        placeholder={t("common.example", { example: OCTOBOARDD_EXAMPLE })}
        dir="ltr"
        errorMessage={shown(error)}
        autoFocus
      />
    </Dialog>
  );
}
