import React, { useEffect, useId, useRef, useState } from "react";

import { FailureAlert } from "../../components/alerts/FailureAlert";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../../components/announce/StatusAnnouncer";
import { appDataRecovery } from "../../errors/errorRecovery";
import { errorPresentation, type ErrorPresentation } from "../../errors/errorPresentation";
import { defaultAccountName, servedAgents, listedAgents } from "../../agents/agents";
import { ProviderMessage } from "../../agents/ProviderMessage";
import { providerConfigDir, providerLabel } from "../../agents/providers";
import { agentIconSetupOptions } from "../../components/icons/AgentIcon";
import { TILDE_PARAMS } from "../../components/text/literalParams";
import { Dialog } from "../../components/dialog/Dialog";
import { useDialogAction, useRefocusIfLost, useSubmitValidation } from "../../components/dialog/dialogHooks";
import { useDirectoryBrowse } from "../../components/dialog/DirectoryPicker";
import { OptionSelect } from "../../components/form/OptionSelect";
import { TextInput } from "../../components/form/TextInput";
import { isAbsoluteOrHome } from "../../format/hostPath";
import type { MessageKey } from "../../i18n/catalog";
import { isolate } from "../../i18n/isolate";
import { InlineReference } from "../../i18n/InlineReference";
import { Message, useT } from "../../i18n/react";
import type { Account, Agent } from "../../protocol";
import { useDaemon, useDaemonStore, useProviders } from "../../store";
import { useHostPaths } from "../../components/hooks/useHostPaths";
import { ProviderName } from "../../components/text/ProviderName";
import { nameCollision, nextConfigDir, type NameCollision } from "./agentAccounts";

/** What each kind of name collision says. */
const COLLISION_MESSAGE = {
  account: "settings.accounts.nameTaken",
  default: "settings.accounts.nameTakenDefault",
  reserved: "settings.accounts.nameReserved",
} as const satisfies Record<NameCollision["kind"], MessageKey>;

/** Adds an account, or edits `editing`'s name and directory. The agent is chosen only when adding:
 * a directory belongs to one agent's layout, so an existing account never changes it, and the
 * default account's agent is fixed too. With `renameDefault` it edits the name of the agent's
 * default account instead, which has no directory to edit and takes a blank name as clearing it. */
export function AccountDialog({
  account: editing,
  agent: initialAgent,
  renameDefault,
  onClose,
}: {
  account?: Account;
  renameDefault?: boolean;
  /** The agent a new account starts on. */
  agent: Agent;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const { style } = useHostPaths();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const addedAgents = useDaemonStore((s) => s.settings.added_agents);
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  const providers = useProviders();
  const [agent, setAgent] = useState<Agent>(editing?.agent ?? initialAgent);
  const [name, setName] = useState(editing?.name ?? (renameDefault ? defaultNames?.[initialAgent] ?? "" : ""));
  const [typedConfigDir, setConfigDir] = useState<string | undefined>(editing?.config_dir);
  const [directoryListing, setDirectoryListing] = useState<{ home: string; paths: string[] }>();
  const [directoryFailure, setDirectoryFailure] = useState<{ message: ErrorPresentation; attempt: number }>();
  const [listingAttempt, setListingAttempt] = useState(0);
  const [listingPending, setListingPending] = useState(true);
  const { busy, run } = useDialogAction(undefined, t(editing || renameDefault ? "settings.accounts.dialog.saveFailedTitle" : "settings.accounts.dialog.addFailedTitle"), renameDefault ? appDataRecovery : undefined);
  const { shown, attempt } = useSubmitValidation();
  const directoryField = useRef<HTMLDivElement>(null);
  const directoryFailureId = useId();
  const showDirectoryFailure = !editing && !renameDefault && typedConfigDir === undefined && directoryFailure;
  useRefocusIfLost(() => listingAttempt > 0 ? directoryField.current?.querySelector("input") : null, [directoryListing]);

  useEffect(() => {
    if (editing || renameDefault) return;
    let active = true;
    void request({ type: "list_dir", path: "~" }).then((event) => {
      if (!active) return;
      setDirectoryListing({ home: event.path, paths: event.entries.map((entry) => entry.path) });
      setDirectoryFailure(undefined);
      setListingPending(false);
    }).catch((error: unknown) => {
      if (!active) return;
      setDirectoryFailure({ message: errorPresentation(error), attempt: listingAttempt });
      setListingPending(false);
    });
    return () => { active = false; };
  }, [request, editing, renameDefault, listingAttempt]);

  const defaultConfigDir = providerConfigDir(providers, agent);
  const configDir = typedConfigDir ?? (directoryListing && defaultConfigDir !== undefined
    ? nextConfigDir(defaultConfigDir, [
      ...directoryListing.paths,
      ...accounts.map((account) => account.config_dir),
      ...Array.from(agentAvailability.values()).flatMap((availability) => availability.default_account_dir ? [availability.default_account_dir] : []),
    ], directoryListing.home, style)
    : "");
  const directoryBrowse = useDirectoryBrowse(configDir, setConfigDir);

  const collision = name.trim() === "" && renameDefault
    ? undefined
    : nameCollision(name, agent, accounts, renameDefault ? null : defaultAccountName(t, defaultNames, agent), editing?.id);
  const nameError = !name && !renameDefault
    ? t("dialog.nameRequired")
    : collision && t(COLLISION_MESSAGE[collision.kind], { name: isolate(collision.name), agent: isolate(providerLabel(t, providers, agent)) });
  const dirError = isAbsoluteOrHome(configDir, style) ? undefined : "notAbsolute";

  const submit = () => {
    if (!attempt(nameError, renameDefault ? undefined : dirError)) return;
    // Whether another account uses the directory is judged by the daemon, which alone knows the home
    // directory and the default account's resolved one. Its refusal comes after the submit, so it is
    // the error alert dialog's, like every other failure of the action.
    void run(async () => {
      if (renameDefault) {
        await request({ type: "set_default_account_name", agent, name });
      } else if (editing) {
        await request({ type: "update_account", account: editing.id, name, config_dir: configDir });
      } else {
        await request({ type: "create_account", agent, name, config_dir: configDir });
      }
      onClose();
    });
  };

  return (
    <>
      <Dialog
        title={t(renameDefault ? "settings.accounts.dialog.renameDefault" : editing ? "settings.accounts.dialog.edit" : "settings.accounts.add")}
        onClose={onClose}
        submitLabel={editing || renameDefault ? t("common.save") : t("common.add")}
        busy={busy}
        onSubmit={submit}
      >
        {renameDefault && <p className="text-sm"><Message id="settings.accounts.dialog.renameDefaultSubject" params={{ agent: <ProviderName agent={agent} /> }} /></p>}
        {editing && <p className="text-sm"><Message id="settings.accounts.dialog.editSubject" params={{ agent: <ProviderName agent={editing.agent} />, name: <InlineReference dir="auto" wrap>{editing.name}</InlineReference> }} /></p>}
        {!editing && !renameDefault && (
          <OptionSelect
            label={t("settings.accounts.agent")}
            // Availability is shown but deliberately not enforced: an account may be set up before
            // its agent is installed, so every added agent is selectable; one not added is not listed.
            options={agentIconSetupOptions(t, providers, agentAvailability, listedAgents(addedAgents, servedAgents(providers)))}
            value={agent}
            onChange={setAgent}
          />
        )}
        <TextInput
          label={t("common.name")}
          value={name}
          onChange={setName}
          placeholder={renameDefault ? t("settings.accounts.defaultName") : t("settings.accounts.nameExample")}
          description={renameDefault ? t("settings.accounts.defaultNameHint") : undefined}
          errorMessage={shown(nameError) && (collision
            ? <Message id={COLLISION_MESSAGE[collision.kind]} params={{ name: <InlineReference dir="auto" wrap>{collision.name}</InlineReference>, agent: <ProviderName agent={agent} /> }} />
            : nameError)}
          autoFocus
        />
        {!renameDefault && <div ref={directoryField} className="contents"><TextInput
          label={t("settings.accounts.configDir")}
          value={configDir}
          onChange={setConfigDir}
          dir="ltr"
          placeholder={defaultConfigDir}
          description={<ProviderMessage providers={providers} id={agent} name="dirHint" fallback="settings.accounts.dirHint" />}
          describedBy={showDirectoryFailure ? directoryFailureId : undefined}
          errorMessage={shown(dirError) && <Message id="settings.accounts.dirNotAbsolute" params={TILDE_PARAMS} />}
          trailing={directoryBrowse.browseButton()}
        /></div>}
        {showDirectoryFailure && <FailureAlert
          textId={directoryFailureId}
          title={t("directoryPicker.failed")}
          message={directoryFailure.message}
          announce
          announceDelayMs={AFTER_FOCUS_MOVE_DELAY_MS}
          attempt={directoryFailure.attempt}
          pending={listingPending}
          onRetry={() => {
            setListingPending(true);
            setListingAttempt((value) => value + 1);
          }}
        />}
      </Dialog>
      {directoryBrowse.picker}
    </>
  );
}
