import { Plus, Server } from "lucide-react";
import React, { useEffect, useId, useRef } from "react";

import { focusLanding } from "../components/focus/regionFocus";
import { MODAL_OPEN } from "../components/focus/selectors";
import { ContentMain } from "../components/panel/ContentMain";
import { CenteredEmptyPanel } from "../components/panel/EmptyPanel";
import { FadedScroller, useFadeRegion } from "../components/scroll/scrollFade";
import { StatusBadge } from "../components/status/StatusBadge";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import { ringForAutomaticFocus } from "../input/lastInput";
import { Button, Spinner } from "../primitives";
import { SettingRow } from "../settings/SettingRow";
import { CONNECTION_STATE_WORDS, connectionState } from "./connectionState";
import { FormCard } from "../components/card/FormCard";
import { RemoteDaemonForm } from "./RemoteDaemonForm";
import { RemoteDeviceKeyCard } from "./RemoteDeviceKeyCard";
import { useRemoteDevices } from "./RemoteDevices";

/**
 * The Remote devices page's content area: the selected device's settings as a form (`RemoteDaemonForm`) under a header
 * with its name, how it is connected, where it connects to and Connect (Open once it is connected), then whether this
 * computer's own key is installed on its server, with its switch (`RemoteDeviceKeyCard`), and Remove, the one
 * destructive action, in a row of its own at the bottom; or a new device's blank form, titled Add remote daemon; or,
 * with none saved, the empty state alone, centred. Nothing but the frame is drawn until the list has been read.
 * Switching to another device asks first when the form holds changes not saved (`useUnsavedChanges`), and drops them once
 * the user discards. A connected daemon opens in a
 * window of its own and this page stays as it is. Opening the page moves focus into it, once the list is read and no
 * dialog or drawer is in front.
 */
export function RemoteDevicePage({ drawerOpen = false }: { drawerOpen?: boolean }): React.ReactElement {
  const t = useT();
  // The page's scroller is a region named by what the page is of: the device it shows, or the new one's form.
  const titleId = useId();
  const { entries, entriesRead, statuses, selectedId, adding, startAdd, cancelAdd, save, connecting, connect, askRemove } = useRemoteDevices();
  const region = useFadeRegion({ labelledBy: entriesRead ? titleId : undefined });
  const pageRef = useRef<HTMLElement>(null);
  const entry = entries.find((one) => one.id === selectedId);

  // Opening the page moves focus into it, once the list has been read and so there is a control to land on; not over a
  // dialog or a control of the page that already has it. A mouse open shows no ring (`ringForAutomaticFocus`).
  const landed = useRef(false);
  useEffect(() => {
    if (!entriesRead || landed.current || !pageRef.current) return;
    // Not while the narrow window's drawer is open over the page, which has the focus; it lands once the drawer closes.
    if (drawerOpen) return;
    landed.current = true;
    // Nor over a field that already has the focus (Add placed it, as a drawer's choice does).
    if (document.querySelector(MODAL_OPEN) || pageRef.current.contains(document.activeElement)) return;
    ringForAutomaticFocus();
    // A new device's form lands on its opening field, which its autofocus could not take under the drawer.
    const opening = adding ? pageRef.current.querySelector<HTMLElement>("form input") : null;
    if (opening) opening.focus();
    else focusLanding(pageRef.current);
  }, [entriesRead, drawerOpen]);

  let body: React.ReactNode = null;
  if (entriesRead) {
    if (adding) {
      body = (
        <>
          <h1 id={titleId} className="text-2xl font-semibold">{t("remoteDaemons.add")}</h1>
          <RemoteDaemonForm key="new" autoFocus onSave={save} onCancel={cancelAdd} />
        </>
      );
    } else if (entries.length === 0) {
      // Nothing saved: the empty state alone, centred in the area, with the line that says what a remote daemon is. Its
      // heading is the page's.
      return (
        <ContentMain ref={pageRef} data-region="devices" className={`min-h-0 flex-col ${region.insetRing}`}>
          <FadedScroller region={region} className="flex min-h-0 flex-1 flex-col p-8">
            <CenteredEmptyPanel icon={Server} headingLevel={1} headingId={titleId} message={t("remoteDaemons.empty")} description={t("remoteDaemons.empty.description")} action={{ label: t("remoteDaemons.add"), icon: Plus, onPress: startAdd }} />
          </FadedScroller>
        </ContentMain>
      );
    } else if (entry) {
      const status = statuses.find((one) => one.id === entry.id);
      const state = status && connectionState(status);
      body = (
        <>
          <header className="flex items-start justify-between gap-4">
            <div className="flex min-w-0 flex-col gap-1">
              <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                <h1 id={titleId} dir="auto" className="min-w-0 text-2xl font-semibold [overflow-wrap:anywhere]">{entry.name}</h1>
                {state && (
                  <StatusBadge color={state === "reconnecting" ? "warning" : "success"} variant="soft">
                    {state === "reconnecting" ? (
                      <span className="inline-flex items-center gap-1 align-top">
                        <Spinner size="compact" color="current" />
                        {t(CONNECTION_STATE_WORDS[state])}
                      </span>
                    ) : t(CONNECTION_STATE_WORDS[state])}
                  </StatusBadge>
                )}
              </div>
              <p dir="ltr" className="align-match-parent font-mono text-sm text-app-muted [overflow-wrap:anywhere]">
                {`${entry.user}@${entry.host}`}
                {/* A break opportunity before the port, which itself stays whole. */}
                <wbr />
                <span className="whitespace-nowrap">{`:${entry.port}`}</span>
              </p>
            </div>
            <Button size="sm" variant="primary" focusOnPress={false} className="shrink-0" pending={connecting === entry.id} disabled={connecting !== undefined && connecting !== entry.id} describedBy="" label={t(state ? "remoteDaemons.openLabel" : "remoteDaemons.connectLabel", { name: isolate(entry.name) })} onPress={() => connect(entry)}>
              {/* A connected one is only brought forward, or opened in the browser again. */}
              {t(state ? "remoteDaemons.open" : "remoteDaemons.connect")}
            </Button>
          </header>
          <RemoteDaemonForm key={entry.id} entry={entry} onSave={save} />
          <RemoteDeviceKeyCard entry={entry} />
          <FormCard>
            {/* The row's own padding is the card's. */}
            <div className="-my-4">
              <SettingRow label={t("remoteDaemons.removeTitle")} compact>
                <Button size="sm" variant="danger-soft" focusOnPress={false} describedBy="" label={t("remoteDaemons.removeLabel", { name: isolate(entry.name) })} onPress={() => askRemove(entry)}>
                  {t("common.remove")}
                </Button>
              </SettingRow>
            </div>
          </FormCard>
        </>
      );
    }
  }

  return (
    <ContentMain ref={pageRef} data-region="devices" className={`min-h-0 flex-col ${region.insetRing}`}>
      <FadedScroller region={region} className="flex min-h-0 flex-1 flex-col">
        <div className="mx-auto flex w-full min-w-0 max-w-3xl flex-col gap-6 px-4 pt-10 pb-10 sm:px-8 sm:pt-12">{body}</div>
      </FadedScroller>
    </ContentMain>
  );
}
