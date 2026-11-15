import { Check, ExternalLink, RefreshCw, Server, Unplug } from "lucide-react";
import React, { useRef } from "react";

import { alertFailure } from "../components/alerts/raiseError";
import { TitledControl } from "../components/hint/TitledControl";
import { ActionMenu } from "../components/menus/ActionMenu";
import { useListMotion } from "../components/motion/useListMotion";
import { FadedScroller } from "../components/scroll/scrollFade";
import { isolate } from "../i18n/isolate";
import { useCurrentLanguage, useT } from "../i18n/react";
import { textDirection } from "../i18n/languages";
import type { RemoteDaemonEntry, RemoteDaemonsCapability } from "../platform";
import { usePlatform } from "../platform/react";
import { Badge, BadgeAnchor, Button } from "../primitives";
import { avatarFill } from "./ConsoleAvatar";
import { CHROME_BUTTON_FILLS } from "./ChromeButton";
import { remoteDaemonsOffered } from "../remoteDaemons/remoteDaemonsOffered";
import { CONNECTION_STATE_WORDS, connectionState, type ConnectionState } from "../remoteDaemons/connectionState";
import { useRemoteDaemons } from "../remoteDaemons/useRemoteDaemons";

/** What the group keeps while it has a tile, in rem: the groove and its padding (0.25), one tile with its padding
 * (2.5) and the scroller's (0.25 each side, of which one is counted with the groove's). */
const GROUP_MINIMUM_HEIGHT = "min-h-[3.25rem]";

/**
 * The rail's group of the remote daemons that are connected, under the consoles and a groove: one tile per connected
 * entry, whether its window is open or the connection is held for the browser. Entries are added, edited, removed and
 * connected from the connect page of a new remote window, so the rail has none of that; a daemon that is not connected
 * has no tile, and with none connected there is no group and no groove. Only in the application's own window
 * (`remoteDaemonsOffered`). In a window too short for everything this group gives way first, down to one tile,
 * scrolling on its own; the consoles keep their height for as long as they can (their shrink factor is 1, this group's
 * a thousand: factors summing below 1 would share out only that fraction of what overflows), and the bottom group never
 * leaves the rail.
 */
export function RailRemoteDaemons(): React.ReactElement | null {
  const platform = usePlatform();
  return remoteDaemonsOffered(platform) ? <ConnectedRemoteDaemons capability={platform.remoteDaemons!} /> : null;
}

function ConnectedRemoteDaemons({ capability }: { capability: RemoteDaemonsCapability }): React.ReactElement | null {
  const t = useT();
  const { entries, statuses, reload } = useRemoteDaemons(capability);
  const listRef = useListMotion<HTMLDivElement>();
  const connected = entries.flatMap((entry) => {
    const status = statuses.find((one) => one.id === entry.id);
    return status ? [{ entry, state: connectionState(status) }] : [];
  });
  if (connected.length === 0) return null;

  // The shell focuses the daemon's window, or opens the browser again. Anything else it answers (the connection ended
  // since the list was read) goes through the same way as the menus' "Open", which asks for what it needs.
  const open = (entry: RemoteDaemonEntry) =>
    capability
      .connect({ id: entry.id, secret: null, confirmFingerprint: null, action: null, browser: false })
      .then((outcome) => (outcome.outcome === "opened" || outcome.outcome === "opened_in_browser" ? undefined : capability.openWindow(entry.id)))
      .catch(alertFailure({ title: t("remoteDaemons.error.failed.title") }));
  const disconnect = (entry: RemoteDaemonEntry) =>
    capability.disconnect(entry.id).then(reload, alertFailure({ title: t("remoteDaemons.disconnectFailedTitle") }));

  return (
    <div role="group" aria-label={t("remoteDaemons.label")} className={`flex w-full shrink-[1000] flex-col items-center px-2 ${GROUP_MINIMUM_HEIGHT}`}>
      <div className="groove-t flex min-h-0 w-full flex-col items-center pt-1">
        {/* The scroller's padding is the room a tile's badge overhangs into, so the edge's fade and clip leave it whole. */}
        <FadedScroller hideScrollBar className="relative flex min-h-0 flex-col items-center overscroll-contain py-0.5">
          <div ref={listRef} className="relative flex flex-col items-center">
            {connected.map(({ entry, state }) => (
              <RemoteDaemonTile key={entry.id} entry={entry} state={state} onOpen={() => void open(entry)} onDisconnect={() => void disconnect(entry)} />
            ))}
          </div>
        </FadedScroller>
      </div>
    </div>
  );
}

/** One connected entry: a round avatar with the remote mark (the glyph the title bar of a remote window carries;
 * consoles have the network one) on a fill picked from its name, with a badge on its corner, the check for a connection
 * that is up and the refresh while it is being brought back. A press focuses its window or opens the browser again. A
 * right-click offers Open and Disconnect, from a menu whose trigger is not drawn, as a console's tile does. */
function RemoteDaemonTile({ entry, state, onOpen, onDisconnect }: {
  entry: RemoteDaemonEntry;
  state: ConnectionState;
  onOpen: () => void;
  onDisconnect: () => void;
}): React.ReactElement {
  const t = useT();
  const rtl = textDirection(useCurrentLanguage()) === "rtl";
  const tileRef = useRef<HTMLDivElement>(null);
  const address = `${entry.user}@${entry.host}:${entry.port}`;
  const stateWords = t(CONNECTION_STATE_WORDS[state]);
  return (
    <div ref={tileRef} data-list-motion={entry.id} className="flex px-1 pb-1">
      <BadgeAnchor>
        <TitledControl
          placement="end"
          title={
            <div className="flex min-w-0 flex-col gap-0.5">
              <div dir="auto" className="min-w-0 font-medium [overflow-wrap:anywhere]">{entry.name}</div>
              <div dir="ltr" className="min-w-0 font-mono text-app-muted [overflow-wrap:anywhere]">{address}</div>
              <div>{stateWords}</div>
            </div>
          }
        >
          <Button
            iconOnly
            size="sm"
            variant="ghost"
            label={t("remoteDaemons.tile.label", { name: isolate(entry.name), address: isolate(address), state: stateWords })}
            // The markup tooltip repeats the name, which TitledControl only detects for plain text.
            describedBy=""
            focusOnPress={false}
            onPress={onOpen}
            className={`size-9 min-w-0 rounded-lg p-0 ${CHROME_BUTTON_FILLS}`}
          >
            <span aria-hidden="true" className={`flex size-7 shrink-0 items-center justify-center rounded-full text-avatar-foreground ${avatarFill(entry.name)}`}>
              <Server className="size-[50%]" />
            </span>
          </Button>
        </TitledControl>
        <Badge color={state === "reconnecting" ? "warning" : "success"} placement={rtl ? "bottom-left" : "bottom-right"}>
          {state === "reconnecting" ? <RefreshCw strokeWidth={2.5} size={10} /> : <Check strokeWidth={2.5} size={10} />}
        </Badge>
      </BadgeAnchor>
      <ActionMenu
        className="hidden"
        label={t("remoteDaemons.menu.label", { name: isolate(entry.name) })}
        tooltip={false}
        items={[
          { label: t("remoteDaemons.open"), icon: ExternalLink, onClick: onOpen },
          { label: t("remoteDaemons.disconnect"), icon: Unplug, onClick: onDisconnect },
        ]}
        contextTargetRef={tileRef}
      />
    </div>
  );
}
