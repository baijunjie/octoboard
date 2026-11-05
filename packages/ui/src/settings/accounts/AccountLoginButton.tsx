import { LogIn } from "lucide-react";
import React, { useEffect, useRef, useState } from "react";

import { ICON_TRIGGER_BASE_CLASS, ICON_TRIGGER_CLASS } from "../../components/controls/iconTrigger";
import { TitledControl } from "../../components/hint/TitledControl";
import { isolate } from "../../i18n/isolate";
import { useT } from "../../i18n/react";
import type { Agent } from "../../protocol";
import type { QuotaReading } from "../../quota/useAccountQuota";
import { loginStateOf } from "../../accounts/quotaFormat";
import { useDaemonStore } from "../../store";
import { afterFrames } from "../../components/focus/afterFrames";
import { canTakeFocus } from "../../components/focus/regionFocus";
import { useLatest } from "../../components/hooks/useLatest";
import { ringForAutomaticFocus } from "../../input/touchInput";
import { accountLoginOf, loginIsActive } from "../../accounts/accountRecords";
import { Button, onFocusScopeRestore } from "../../primitives";

/** Told, as the login dialog a Log in button opened closes, whether the login succeeded, and the cancel closing it
 * sent, if any. */
export type ReturnLoginFocus = (succeeded: boolean, cancelling?: Promise<void>) => void;

/**
 * Where focus goes once the login dialog a Log in button opened has closed, which React Aria's restore alone does not
 * settle: it puts focus back on that button, and the button is not always there to take it. A login that succeeded, or
 * the account being turned off, can take the button away (the row's is shown only while the account reads as signed out
 * and is on), before or after the restore, so focus goes to `fallback` instead; so it does when a cancelled login turns
 * out to have succeeded first. Closing the dialog otherwise cancels a login still running, and the button is
 * unavailable while the daemon has a login running — including one it registers only after the dialog has gone. Focus
 * goes to the button once the cancel has been answered and the button is available again, or to `fallback` if it has
 * gone. Either way focus would otherwise end on `<body>`, where VoiceOver reads the whole web area and does not speak
 * the row's status.
 *
 * A removed opener makes React Aria restore to another control in its parent scope. Intercept that automatic
 * restore to choose the row's fallback instead, before it could be mistaken for a deliberate focus move. A pending
 * cancel still waits a frame past the restore before returning to the button once it is available. Focus already
 * held elsewhere is left alone.
 */
function useLoginFocusReturn(
  button: React.RefObject<HTMLElement | null>,
  fallback: () => HTMLElement | null | undefined,
  { active, succeeded }: { active: boolean; succeeded: boolean },
): ReturnLoginFocus {
  const [returning, setReturning] = useState<{ to: "button" | "fallback"; cancelling: boolean }>();
  const latest = useLatest({ fallback, succeeded });
  const restoreGuard = useRef<AbortController | undefined>(undefined);
  useEffect(() => () => restoreGuard.current?.abort(), []);
  useEffect(() => {
    if (!returning || returning.cancelling || (returning.to === "button" && active)) return;
    return afterFrames(2, () => {
      restoreGuard.current?.abort();
      setReturning(undefined);
      const opener = button.current;
      const target = returning.to === "button" && !succeeded && opener ? opener : fallback();
      const focused = document.activeElement;
      if (!target || focused === target || (focused !== document.body && !opener?.contains(focused))) return;
      // The user did not ask for this move, so the destination shows its ring.
      ringForAutomaticFocus();
      target.focus();
    });
  }, [returning, active, succeeded]);
  return (closedOnSuccess, cancelling) => {
    restoreGuard.current?.abort();
    const controller = new AbortController();
    restoreGuard.current = controller;
    const closingDialog = document.activeElement?.closest('[role="dialog"], [role="alertdialog"]');
    onFocusScopeRestore((event) => {
      if (event.defaultPrevented || (!closedOnSuccess && button.current?.isConnected && !latest.current.succeeded)) return;
      const target = latest.current.fallback();
      if (!canTakeFocus(target)) return;
      event.preventDefault();
      controller.abort();
      ringForAutomaticFocus();
      target.focus();
    }, controller.signal);
    document.addEventListener("focusin", (event) => {
      if (!(event.target instanceof Node) || !closingDialog?.contains(event.target)) controller.abort();
    }, { capture: true, signal: controller.signal });
    const next = { to: closedOnSuccess ? "fallback" : "button", cancelling: cancelling !== undefined } as const;
    setReturning(next);
    void cancelling?.then(() => setReturning((current) => (current === next ? { ...next, cancelling: false } : current)));
  };
}

/** The Log in button of an account: an icon with a tooltip and a name that says whose account, looking as the row's
 * actions menu button does, shown in the table row's Status cell while the account reads as not logged in and is not
 * turned off (`turnedOff`, where the cell says "Disabled" instead), and nothing otherwise. While a login of the account
 * runs it is unavailable, not natively disabled: it keeps focus, is marked `aria-disabled`, takes no hover tint or
 * press shrink, gives "Logging in…" as its tooltip and its description (its name stays the action), and a press does
 * nothing (see "Buttons" in `docs/ui-standard.md`). Focus comes back after the login dialog closes as
 * `useLoginFocusReturn` settles it (`fallback` is where it goes when the button is gone). */
export function AccountLoginButton({ agent, account, name, quota, turnedOff, onLogin, fallback }: {
  agent: Agent;
  account: string | null;
  name: string;
  quota: QuotaReading;
  turnedOff: boolean;
  onLogin: (returnFocus: ReturnLoginFocus) => void;
  fallback: () => HTMLElement | null | undefined;
}): React.ReactElement | null {
  const t = useT();
  const login = useDaemonStore((s) => accountLoginOf(s.accountLogins, agent, account));
  const active = loginIsActive(login);
  const button = useRef<HTMLButtonElement>(null);
  const returnFocus = useLoginFocusReturn(button, fallback, { active, succeeded: login?.status === "succeeded" });
  if (turnedOff || loginStateOf(quota) !== "out") return null;
  return (
    <TitledControl title={active ? t("settings.accounts.login.starting") : t("settings.accounts.login")}>
      <Button
        ref={button}
        variant="ghost"
        focusOnPress={false}
        unavailable={active ? t("settings.accounts.login.starting") : undefined}
        label={t("settings.accounts.login.open", { name: isolate(name) })}
        className={active ? ICON_TRIGGER_BASE_CLASS : ICON_TRIGGER_CLASS}
        onPress={() => {
          if (active) return;
          const focused = document.activeElement;
          onLogin(focused === document.body || button.current?.contains(focused) ? returnFocus : () => {});
        }}
      >
        <LogIn aria-hidden="true" className="size-4" />
      </Button>
    </TitledControl>
  );
}
