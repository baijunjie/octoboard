import { useCurrentLanguage, useT } from "../i18n/react";
import type { Session } from "../protocol";
import { sessionRowModel, type SessionRowModel } from "../sessions/sessionRowModel";
import type { ShownStatusKind } from "../sessions/sessionStatus";
import { useDaemonStore, useSessionLaunch } from "../store";

/** The model of a session row, with the accounts and the launch read from the store. */
export function useSessionRowModel(session: Session, owners: readonly (Session | undefined)[], status: ShownStatusKind): SessionRowModel {
  const t = useT();
  const language = useCurrentLanguage();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  const launch = useSessionLaunch(session);
  return sessionRowModel(t, language, session, owners, status, { accounts, defaultNames, launch });
}
