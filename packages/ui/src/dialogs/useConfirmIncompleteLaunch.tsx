import React, { useEffect, useRef, useState } from "react";

import { incompleteAccount } from "../accountChoices";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import { openSettingsFromAnywhere } from "../settings/settingsLauncher";
import { useDaemon } from "../store";
import { ConfirmDialog } from "./ConfirmDialog";

export type ConfirmLaunch = (account: string | null | undefined, launch: () => void | Promise<void>) => Promise<void>;

/** A question on screen: what it asks about, and how to settle the promise `confirmLaunch` returned. */
interface Asked {
  id: string;
  name: string;
  settle: (launched?: Promise<void>) => void;
  launch: () => void | Promise<void>;
}

/**
 * The check every control that starts a session of an account the user chose goes through (a new
 * session, a resume, a switch onto the account, a handoff onto it, a quick create).
 * `confirmLaunch` runs `launch` at once, unless `account` is one whose last sharing change did not
 * finish and none runs now (`incompleteAccount`): then it asks first, and `launch` runs only on
 * Confirm, while the close button and Escape send nothing. The other button, View in Settings,
 * cancels the same way and opens Settings on the account's sharing switch, where the problem is
 * shown; `closeOwner` is the dialog the caller renders the question in, which closes with it. It
 * resolves once `launch` has run (or rejects with its failure, as without the question) or the
 * question was cancelled. The question closes on Confirm, before `launch` runs, so a caller shows
 * its own busy state while it waits. A second `confirmLaunch` while a question is open cancels
 * that one, settling its promise, and asks its own. A caller inside a dialog renders
 * `confirmation` in that dialog's children, so its focus scope nests in the dialog's; one with no
 * dialog renders it at its own level. A launch the daemon starts by itself (a sharing change's own
 * resumes, a restart) does not come through here.
 */
export function useConfirmIncompleteLaunch(closeOwner?: () => void): {
  confirmLaunch: ConfirmLaunch;
  confirmation: React.ReactElement | undefined;
} {
  const t = useT();
  const { store } = useDaemon();
  const [asked, setAsked] = useState<Asked>();
  // The question is also held outside state, so that a request arriving before the render that
  // shows the previous one still finds it to cancel.
  const current = useRef<Asked | undefined>(undefined);
  const show = (next: Asked | undefined) => {
    current.current = next;
    setAsked(next);
  };
  useEffect(() => () => current.current?.settle(), []);
  const confirmLaunch: ConfirmLaunch = (account, launch) => {
    const { settings, accountSharing } = store.getState();
    const incomplete = incompleteAccount(settings.accounts, accountSharing, account);
    if (!incomplete) return Promise.resolve(launch());
    current.current?.settle();
    return new Promise<void>((resolve) => {
      show({ id: incomplete.id, name: incomplete.name, launch, settle: (launched) => resolve(launched) });
    });
  };
  const cancel = () => {
    current.current?.settle();
    show(undefined);
  };
  const confirmation = asked && (
    <ConfirmDialog
      title={t("dialog.incompleteLaunch.title", { account: isolate(asked.name) })}
      message={t("dialog.incompleteLaunch.message")}
      confirmLabel={t("dialog.incompleteLaunch.confirm")}
      onCancel={cancel}
      hideCancel
      otherAction={{
        label: t("dialog.incompleteLaunch.viewSettings"),
        onPress: () => {
          cancel();
          openSettingsFromAnywhere("accounts", { account: asked.id });
          closeOwner?.();
        },
      }}
      onConfirm={async () => {
        show(undefined);
        asked.settle(Promise.resolve(asked.launch()));
      }}
    />
  );
  return { confirmLaunch, confirmation };
}
