import { LogIn } from "lucide-react";
import React, { useEffect, useId, useRef, useState } from "react";

import { ICON_TRIGGER_BASE_CLASS, ICON_TRIGGER_CLASS } from "../components/ActionMenu";
import { Button } from "../components/Button";
import { TitledControl } from "../components/TitledControl";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import type { Agent } from "../protocol";
import type { QuotaReading } from "../quota/useAccountQuota";
import { loginStateOf } from "../quota/quotaFormat";
import { useDaemonStore } from "../store";
import { ringForAutomaticFocus } from "../touchInput";
import { accountLoginOf, loginIsActive } from "./AccountLogin";

/** Told, as the login dialog a Log in button opened closes, whether the login succeeded, and the cancel closing it
 * sent, if any. */
export type ReturnLoginFocus = (succeeded: boolean, cancelling?: Promise<void>) => void;

/**
 * Where focus goes once the login dialog a Log in button opened has closed, which React Aria's restore alone does not
 * settle: it puts focus back on that button, and the button is not always there to take it. A login that succeeded can
 * take the button away (the row's is shown only while the account reads as signed out), before or after the restore,
 * so focus goes to `fallback` instead; so it does when a cancelled login turns out to have succeeded first. Closing the
 * dialog otherwise cancels a login still running, and the button is unavailable while the daemon has a login running —
 * including one it registers only after the dialog has gone. Focus goes to the button once the cancel has been
 * answered and the button is available again, or to `fallback` if it has gone. Either way focus would otherwise end on
 * `<body>`, where VoiceOver reads the whole web area and does not speak the row's status.
 *
 * The move waits a frame past the restore, which runs in the frame after the dialog unmounts, and acts only where the
 * restore left focus on `<body>` or on a button that is going: a restore that landed right, or on another control,
 * is left alone. A restore React Aria defers further for a screen reader's virtual cursor acts only while focus is
 * still on `<body>`, so it does not undo this move.
 */
function useLoginFocusReturn(
  button: React.RefObject<HTMLElement | null>,
  fallback: () => HTMLElement | null | undefined,
  { active, succeeded }: { active: boolean; succeeded: boolean },
): ReturnLoginFocus {
  const [returning, setReturning] = useState<{ to: "button" | "fallback"; cancelling: boolean }>();
  useEffect(() => {
    if (!returning || returning.cancelling || (returning.to === "button" && active)) return;
    let frame = requestAnimationFrame(() => {
      frame = requestAnimationFrame(() => {
        setReturning(undefined);
        const opener = button.current;
        const target = returning.to === "button" && !succeeded && opener ? opener : fallback();
        const focused = document.activeElement;
        if (!target || focused === target || (focused !== document.body && !opener?.contains(focused))) return;
        // The user did not ask for this move, so the destination shows its ring.
        ringForAutomaticFocus();
        target.focus();
      });
    });
    return () => cancelAnimationFrame(frame);
  }, [returning, active, succeeded]);
  return (closedOnSuccess, cancelling) => {
    const next = { to: closedOnSuccess ? "fallback" : "button", cancelling: cancelling !== undefined } as const;
    setReturning(next);
    void cancelling?.then(() => setReturning((current) => (current === next ? { ...next, cancelling: false } : current)));
  };
}

/** `ICON_TRIGGER_CLASS` without its hover tint and press shrink, for the button while it is unavailable. HeroUI turns
 * pointer events off for `aria-disabled`, so they are turned back on for the tooltip to open (as `RowIconButton`'s
 * unavailable state does), and the ghost button's own hover and pressed fills are covered by a transparent one. */
const UNAVAILABLE_CLASS = `${ICON_TRIGGER_BASE_CLASS} bg-transparent pointer-events-auto data-[pressed=true]:[transform:none] active:[transform:none]`;

/** The Log in button of an account: an icon with a tooltip and a name that says whose account, looking as the row's
 * actions menu button does, shown in the table row's Status cell and in the details dialog's header while the account
 * reads as not logged in, and nothing otherwise. While a login of the account runs it is unavailable, not natively
 * disabled: it keeps focus, is marked `aria-disabled`, takes no hover tint or press shrink, gives "Logging in…" as its
 * tooltip and its description (its name stays the action), and a press does nothing (see "Buttons" in
 * `docs/ui-accessibility.md`). Focus comes back after the login dialog closes as `useLoginFocusReturn` settles it
 * (`fallback` is where it goes when the button is gone). */
export function AccountLoginButton({ agent, account, name, quota, onLogin, fallback }: {
  agent: Agent;
  account: string | null;
  name: string;
  quota: QuotaReading;
  onLogin: (returnFocus: ReturnLoginFocus) => void;
  fallback: () => HTMLElement | null | undefined;
}): React.ReactElement | null {
  const t = useT();
  const login = useDaemonStore((s) => accountLoginOf(s.accountLogins, agent, account));
  const active = loginIsActive(login);
  const button = useRef<HTMLButtonElement>(null);
  const reasonId = useId();
  const returnFocus = useLoginFocusReturn(button, fallback, { active, succeeded: login?.status === "succeeded" });
  if (loginStateOf(quota) !== "out") return null;
  return (
    <TitledControl title={active ? t("settings.accounts.login.starting") : t("settings.accounts.login")}>
      <Button
        ref={button}
        variant="ghost"
        preventFocusOnPress
        aria-disabled={active ? true : undefined}
        aria-describedby={active ? reasonId : undefined}
        aria-label={t("settings.accounts.login.open", { name: isolate(name) })}
        className={active ? UNAVAILABLE_CLASS : ICON_TRIGGER_CLASS}
        onPress={() => { if (!active) onLogin(returnFocus); }}
      >
        {active && <span id={reasonId} className="sr-only">{t("settings.accounts.login.starting")}</span>}
        <LogIn aria-hidden="true" className="size-4" />
      </Button>
    </TitledControl>
  );
}
