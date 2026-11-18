import { CircleArrowUp, Power } from "lucide-react";
import React, { useId, useRef, useState } from "react";

import { useFocusHandoff } from "../components/focus/useFocusHandoff";
import { HeadedTooltip } from "../components/hint/HeadedTooltip";
import { isolate } from "../i18n/isolate";
import { useCurrentLanguage, useT } from "../i18n/react";
import { useHost } from "../platform/react";
import { Button, ProgressCircle, Tooltip } from "../primitives";
import type { OpenSettings } from "../settings/settingsLauncher";
import { installedMessageKey } from "../updates/installedMessageKey";
import { restartBlockedReason } from "../updates/restartBlockedReason";
import { useUpdates } from "../updates/UpdatesProvider";
import { CHROME_BUTTON_FILLS } from "./ChromeButton";

/** An update stays reachable on the rail even after its bottom notice has been put off. */
export function RailUpdateEntry({ focusTerminal, onOpenSettingsAt }: {
  focusTerminal: () => void;
  onOpenSettingsAt: OpenSettings;
}): React.ReactElement | null {
  const updates = useUpdates();
  const t = useT();
  const language = useCurrentLanguage();
  const host = useHost();
  const held = useFocusHandoff(focusTerminal);
  const button = useRef<HTMLButtonElement>(null);
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const progressDescription = useId();
  if (!updates || updates.state.reloadNeeded) return null;
  const { store, state } = updates;
  const installed = state.phase === "installed";
  if (!state.update && !installed) return null;
  const downloading = state.phase === "downloading";
  const version = isolate((installed ? state.installedVersion : state.update?.version) ?? "");
  const restartBlocked = restartBlockedReason(t, state);
  const label = t(installed ? "updates.notice.restart" : state.manual ? "settings.section.updates" : "updates.notice.install");
  const message = installed
    ? t(installedMessageKey(host.updater, "notice", !!state.installedVersion, state.restartBlocked !== undefined), { version })
    : t(downloading ? "updates.notice.downloading" : state.manual ? "updates.notice.manualInstall" : "updates.notice.available", { version });
  const unavailable = installed ? restartBlocked : undefined;
  const percent = state.progress?.total ? Math.max(0, Math.min(state.progress.downloaded / state.progress.total, 1)) : undefined;
  const percentText = percent === undefined ? undefined : new Intl.NumberFormat(language, { style: "percent" }).format(percent);
  const tooltip = <HeadedTooltip heading={label}><span>{message}</span>{downloading && percentText && <span className="tabular-nums">{percentText}</span>}</HeadedTooltip>;
  return (
    <span className="contents"
      onFocus={(event) => { held.onFocus(event); setFocused(true); }}
      onBlur={(event) => { held.onBlur(event); setFocused(false); }}
      onPointerEnter={(event) => { if (event.pointerType === "mouse") setHovered(true); }}
      onPointerLeave={(event) => { if (event.pointerType === "mouse") setHovered(false); }}
    >
      <Button
        ref={button}
        iconOnly
        size="sm"
        variant="ghost"
        label={label}
        describedBy={downloading ? progressDescription : undefined}
        focusOnPress={false}
        pending={downloading || state.phase === "checking" || state.restarting}
        pendingIndicator={downloading ? <ProgressCircle label={message} value={percent === undefined ? undefined : percent * 100} indeterminate={percent === undefined} trackClassName="size-4" /> : undefined}
        dimWhilePending={false}
        tooltip={downloading ? undefined : tooltip}
        tooltipPlacement="end"
        unavailable={unavailable}
        className={`text-accent ${downloading ? "pointer-events-auto" : ""} ${CHROME_BUTTON_FILLS}`}
        onPress={() => {
          if (installed) void store.restart();
          else if (state.manual) onOpenSettingsAt("updates");
          else void store.install();
        }}
      >
        {installed ? <Power className="size-4" /> : <CircleArrowUp className="size-4" />}
      </Button>
      {downloading && <span id={progressDescription} className="sr-only">{message}{percentText && ` ${percentText}`}</span>}
      {/* Pending controls suppress their own hover, so their download detail follows the wrapper's pointer and focus. */}
      <Tooltip trigger="anchored" key={focused ? "focus" : "hover"} content={tooltip} anchor={button} hovered={downloading && (hovered || focused)} immediate={focused} placement="end" />
    </span>
  );
}
