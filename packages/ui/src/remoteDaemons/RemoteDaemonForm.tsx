import React, { useEffect, useId, useRef, useState } from "react";

import { PasteButton } from "../components/controls/PasteButton";
import { useDialogAction } from "../components/dialog/dialogHooks";
import { TextInput } from "../components/form/TextInput";
import { InlineReference } from "../i18n/InlineReference";
import { Message, useT } from "../i18n/react";
import type { RemoteDaemonAuth, RemoteDaemonEntry, RemoteDaemonEntryInput } from "../platform";
import { Button, Radio, RadioGroup } from "../primitives";
import { FormCard } from "./FormCard";
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

/** What the form edits. The pinned host key is shown and cleared, never typed. */
interface Fields {
  name: string;
  host: string;
  port: string;
  user: string;
  auth: RemoteDaemonAuth;
  keyPath: string;
  octoboarddPath: string;
  fingerprint: string | null;
}

/** The fields as `entry` holds them, or as a new entry starts. */
function fieldsOf(entry: RemoteDaemonEntry | undefined): Fields {
  return {
    name: entry?.name ?? "",
    host: entry?.host ?? "",
    port: String(entry?.port ?? DEFAULT_SSH_PORT),
    user: entry?.user ?? "",
    auth: entry?.auth ?? "password",
    keyPath: entry?.keyPath ?? "",
    octoboarddPath: entry?.octoboarddPath ?? "",
    fingerprint: entry?.fingerprint ?? null,
  };
}

/** What each field comes to once saved, so that two inputs that save the same are the same: text trimmed, a blank name
 * the host, the port a number, the key file only where it is the way to sign in, a blank path of `octoboardd` none. */
const SAVED_AS: { [K in keyof Fields]: (fields: Fields) => unknown } = {
  name: (fields) => fields.name.trim() || fields.host.trim(),
  host: (fields) => fields.host.trim(),
  port: (fields) => parsePort(fields.port) ?? fields.port,
  user: (fields) => fields.user.trim(),
  auth: (fields) => fields.auth,
  keyPath: (fields) => (fields.auth === "key" ? fields.keyPath.trim() : null),
  octoboarddPath: (fields) => fields.octoboarddPath.trim() || null,
  fingerprint: (fields) => fields.fingerprint,
};

/** Whether `key` is saved differently by `left` and `right`. */
function differs(key: keyof Fields, left: Fields, right: Fields): boolean {
  return SAVED_AS[key](left) !== SAVED_AS[key](right);
}

/**
 * The form that adds a remote daemon or edits one (`entry`), in the content area of the Remote devices page. It holds a
 * draft of the fields the user changed, over what `entry` holds, so a field the user has not touched follows the saved
 * entry (the shell pins the host key itself, and the path of `octoboardd` is saved from a question), and a field whose
 * draft the saved entry comes to equal follows it again. Save is available while the draft would save something other
 * than the entry holds; Revert drops the draft (Cancel, for a new entry, leaves it: `onCancel`). A field says what is
 * wrong with it once it was edited and left, never merely because another control changed; a Save pressed with
 * something invalid saves nothing, says it of every invalid field and moves focus to the first. A blank name is saved
 * as the host. Connection details `octoboardd connection` printed on the server, pasted into the first field, fill the
 * host, port, user, the path of `octoboardd`, the name where they carry one, and the host key to pin. The pinned host
 * key is shown and can be cleared, so that a server whose key has really changed can be confirmed again on the next
 * connection. This device's own key is offered only to an entry the shell has already switched to it, since the shell
 * installs it at a sign-in with one of the others. `onSave` sends the entry; a rejection is shown and the draft kept. A
 * caller that shows another entry gives the form another `key`, which drops the draft.
 */
export function RemoteDaemonForm({ entry, onSave, onCancel, autoFocus = false }: {
  entry?: RemoteDaemonEntry;
  onSave: (entry: RemoteDaemonEntryInput) => Promise<void>;
  /** Given for a new entry, whose form has no saved state to revert to. */
  onCancel?: () => void;
  /** Puts focus in the form's first field, for a new entry's. */
  autoFocus?: boolean;
}): React.ReactElement {
  const t = useT();
  const fingerprintLabelId = useId();
  const saved = fieldsOf(entry);
  const [draft, setDraft] = useState<Partial<Fields>>({});
  const [details, setDetails] = useState("");
  // The fields that were edited and then left, which say what is wrong with them; and a Save pressed on a form that was
  // not valid, which says it of every field. Nothing says it of a field merely because another control changed.
  const [left, setLeft] = useState<ReadonlySet<string>>(new Set());
  const [edited, setEdited] = useState<ReadonlySet<string>>(new Set());
  const [attempted, setAttempted] = useState(false);
  const formRef = useRef<HTMLFormElement>(null);
  const [focusInvalid, setFocusInvalid] = useState(0);
  const { busy, run } = useDialogAction(undefined, t("remoteDaemons.form.saveFailedTitle"));
  const fields: Fields = { ...saved, ...draft };
  const change = (patch: Partial<Fields>) => {
    setDraft((current) => ({ ...current, ...patch }));
    setEdited((current) => new Set([...current, ...Object.keys(patch)]));
  };
  const leave = (field: string) => {
    if (edited.has(field)) setLeft((current) => new Set([...current, field]));
  };
  // A Save that found something wrong moves focus to the first invalid field, once it has been marked.
  useEffect(() => {
    if (focusInvalid > 0) formRef.current?.querySelector<HTMLElement>("[aria-invalid=true]")?.focus();
  }, [focusInvalid]);
  // A field whose saved value changed to what its draft holds (the draft was saved, or the shell made the same change)
  // follows the entry again. A field whose saved value did not change keeps its draft, whatever else made the entry a
  // new object (an unrelated reload), so what is being typed is never overwritten.
  const previousSaved = useRef(saved);
  useEffect(() => {
    const before = previousSaved.current;
    previousSaved.current = saved;
    setDraft((current) => {
      const kept = (Object.keys(current) as (keyof Fields)[]).filter(
        (key) => !differs(key, before, saved) || differs(key, { ...saved, ...current }, saved),
      );
      return kept.length === Object.keys(current).length ? current : Object.fromEntries(kept.map((key) => [key, current[key]]));
    });
  }, [entry]);
  const dirty = (Object.keys(fields) as (keyof Fields)[]).some((key) => differs(key, fields, saved));
  const portNumber = parsePort(fields.port);
  const hostError = fields.host.trim() ? undefined : t("remoteDaemons.form.host.required");
  const portError = portNumber === undefined ? t("remoteDaemons.form.port.invalid") : undefined;
  const userError = fields.user.trim() ? undefined : t("remoteDaemons.form.user.required");
  const keyPathError = fields.auth === "key" && !fields.keyPath.trim() ? t("remoteDaemons.form.keyPath.required") : undefined;
  const detailsInvalid = !!details.trim() && !parseConnectionDetails(details);
  const detailsError = detailsInvalid ? t("remoteDaemons.form.details.invalid", { command: CONNECTION_COMMAND }) : undefined;
  const valid = !detailsError && !hostError && !portError && !userError && !keyPathError;
  const showError = (field: string, message: string | undefined) => (attempted || left.has(field) ? message : undefined);
  const example = (value: string) => t("common.example", { example: value });

  const changeDetails = (text: string) => {
    setDetails(text);
    setEdited((current) => new Set([...current, "details"]));
    const parsed = parseConnectionDetails(text);
    if (!parsed) return;
    change({
      host: parsed.host,
      port: String(parsed.port),
      user: parsed.user,
      octoboarddPath: parsed.octoboarddPath ?? "",
      // Details that carry no fingerprint leave a pinned one in place: a pin is removed only by Clear.
      ...(parsed.fingerprint ? { fingerprint: parsed.fingerprint } : {}),
      ...(parsed.name ? { name: parsed.name } : {}),
    });
  };

  // What was edited and left, or attempted, belongs to the draft: it goes with it, and with what a save has made of it.
  const forgetMarks = () => {
    setDetails("");
    setEdited(new Set());
    setLeft(new Set());
    setAttempted(false);
  };
  const revert = () => {
    setDraft({});
    forgetMarks();
  };
  // A pasted text is as good as an edited and left field: what is not connection details says so at once.
  const pasteDetails = (text: string) => {
    changeDetails(text.trim());
    setLeft((current) => new Set([...current, "details"]));
  };

  const submit = () => {
    if (!dirty || busy) return;
    if (!valid || portNumber === undefined) {
      setAttempted(true);
      setFocusInvalid((count) => count + 1);
      return;
    }
    void run(async () => {
      await onSave({
        id: entry?.id ?? null,
        name: fields.name.trim() || fields.host.trim(),
        host: fields.host.trim(),
        port: portNumber,
        user: fields.user.trim(),
        auth: fields.auth,
        keyPath: fields.auth === "key" ? fields.keyPath.trim() : null,
        fingerprint: fields.fingerprint,
        octoboarddPath: fields.octoboarddPath.trim() || null,
      });
      forgetMarks();
    });
  };

  return (
    <form
      ref={formRef}
      className="flex flex-col gap-6"
      onSubmit={(event) => {
        event.preventDefault();
        submit();
      }}
    >
      <FormCard title={t("remoteDaemons.form.section.connection")}>
        <TextInput
          label={t("remoteDaemons.form.details")}
          value={details}
          onChange={changeDetails}
          placeholder={example(EXAMPLES.details)}
          description={<Message id="remoteDaemons.form.details.description" params={{ command: <InlineReference monospace>{CONNECTION_COMMAND}</InlineReference> }} />}
          dir="ltr"
          errorMessage={showError("details", detailsError) && <Message id="remoteDaemons.form.details.invalid" params={{ command: <InlineReference monospace>{CONNECTION_COMMAND}</InlineReference> }} />}
          autoFocus={autoFocus}
          onBlur={() => leave("details")}
          trailing={<PasteButton variant="secondary" focusOnPress={false} failedText={t("remoteDaemons.form.details.pasteFailedTitle")} onPaste={pasteDetails} />}
        />
        <TextInput label={t("remoteDaemons.form.name")} value={fields.name} onChange={(name) => change({ name })} placeholder={t("remoteDaemons.form.name.placeholder")} />
        {/* The address in the order it is written, user, host, port, in one row where it fits, and stacked where it does
            not; the row mirrors in right-to-left like any row of controls, the fields' own text staying left to right. */}
        <div className="flex flex-col gap-5 @md:flex-row">
          <div className="min-w-0 @md:w-40 @md:shrink-0">
            <TextInput label={t("remoteDaemons.form.user")} value={fields.user} onChange={(user) => change({ user })} placeholder={example(EXAMPLES.user)} dir="ltr" errorMessage={showError("user", userError)} onBlur={() => leave("user")} />
          </div>
          <div className="min-w-0 flex-1">
            <TextInput label={t("remoteDaemons.form.host")} value={fields.host} onChange={(host) => change({ host })} placeholder={example(EXAMPLES.host)} dir="ltr" errorMessage={showError("host", hostError)} onBlur={() => leave("host")} />
          </div>
          <div className="@md:w-28 @md:shrink-0">
            <TextInput label={t("remoteDaemons.form.port")} value={fields.port} onChange={(port) => change({ port })} placeholder={example(EXAMPLES.port)} dir="ltr" errorMessage={showError("port", portError)} onBlur={() => leave("port")} />
          </div>
        </div>
      </FormCard>
      <FormCard title={t("remoteDaemons.form.section.signIn")}>
        <RadioGroup variant="secondary" label={t("remoteDaemons.form.auth")} value={fields.auth} onChange={(value) => change({ auth: value as RemoteDaemonAuth })}>
          <Radio value="password">{t("remoteDaemons.form.auth.password")}</Radio>
          <Radio value="key">{t("remoteDaemons.form.auth.key")}</Radio>
          {entry?.auth === "device" && (
            <Radio value="device" description={t("remoteDaemons.form.auth.device.description")}>{t("remoteDaemons.form.auth.device")}</Radio>
          )}
        </RadioGroup>
        {fields.auth === "key" && (
          <TextInput
            label={t("remoteDaemons.form.keyPath")}
            value={fields.keyPath}
            onChange={(keyPath) => change({ keyPath })}
            placeholder={example(EXAMPLES.keyPath)}
            description={<Message id="remoteDaemons.form.keyPath.description" params={{ tilde: <InlineReference monospace>~</InlineReference> }} />}
            dir="ltr"
            errorMessage={showError("keyPath", keyPathError)} onBlur={() => leave("keyPath")}
          />
        )}
      </FormCard>
      <FormCard title={t("remoteDaemons.form.section.advanced")}>
        <TextInput
          label={t("remoteDaemons.form.octoboardd")}
          value={fields.octoboarddPath}
          onChange={(octoboarddPath) => change({ octoboarddPath })}
          placeholder={t("remoteDaemons.form.octoboardd.placeholder")}
          description={<Message id="remoteDaemons.form.octoboardd.description" params={{ octoboardd: <InlineReference monospace>octoboardd</InlineReference> }} />}
          dir="ltr"
        />
        {(entry || fields.fingerprint !== null) && (
          <div role="group" aria-labelledby={fingerprintLabelId} className="flex flex-col gap-2 text-sm">
            <span id={fingerprintLabelId} className="font-medium">{t("remoteDaemons.form.fingerprint")}</span>
            {fields.fingerprint === null ? (
              <p className="text-app-muted">{t("remoteDaemons.form.fingerprint.none")}</p>
            ) : (
              <>
                <div className="flex items-center justify-between gap-3">
                  <span dir="ltr" className="min-w-0 align-match-parent font-mono [overflow-wrap:anywhere]">{fields.fingerprint}</span>
                  <Button type="button" size="sm" variant="secondary" onPress={() => change({ fingerprint: null })}>{t("common.clear")}</Button>
                </div>
                <p className="text-app-muted">{t("remoteDaemons.form.fingerprint.description")}</p>
              </>
            )}
          </div>
        )}
      </FormCard>
      <div className="flex items-center gap-2">
        <Button type="submit" size="sm" variant="primary" focusOnPress={false} pending={busy} disabled={!dirty}>
          {t("common.save")}
        </Button>
        {onCancel ? (
          <Button type="button" size="sm" variant="secondary" focusOnPress={false} onPress={onCancel}>{t("common.cancel")}</Button>
        ) : (
          <Button type="button" size="sm" variant="secondary" focusOnPress={false} disabled={!dirty || busy} onPress={revert}>{t("remoteDaemons.form.revert")}</Button>
        )}
      </div>
    </form>
  );
}
