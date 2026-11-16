import { Cable } from "lucide-react";
import React, { useLayoutEffect, useRef } from "react";

import { alertFailure } from "../components/alerts/raiseError";
import { reportExternalLinkFailure } from "../components/text/ExternalLink";
import { focusHandoffTarget, HandoffOnUnmount } from "../components/focus/HandoffOnUnmount";
import { useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { remoteDaemonName } from "../platform/windowKind";
import { ForwardedPorts, PANEL_ATTRIBUTE } from "../remoteDaemons/ForwardedPorts";
import { useRemoteForwards } from "../remoteDaemons/useRemoteForwards";
import { RailCountBadge } from "./RailCountBadge";
import { nestedPopovers } from "./nestedPopover";
import { RailPanelEntry } from "./RailPanelEntry";

const TRIGGER_ID = "rail-forwards";
const OWNER = "rail-forwards";

const nested = nestedPopovers(OWNER);

/** What holds the focus, if it is in `selector` or in a popover opened from the panel. */
const focusHolder = (selector: string): Element | null => document.activeElement?.closest(selector) ?? nested.holding(document.activeElement);

/** The remote window's rail button for the ports forwarded to its daemon's machine, with their count, and the popover it
 * anchors: a row for each forward, pressed to open its address again, with Stop. Built from the Processes button's
 * parts (`RailPanelEntry`, `RailCountBadge`), directly below it, and like it there only while there is something to
 * list: not in any window but a remote one, and not before the first forward or after the last is stopped, which takes
 * the popover with it. */
export function RailForwardsEntry(): React.ReactElement | null {
  const t = useT();
  const platform = usePlatform();
  const capability = platform.remoteForwards;
  const forwards = useRemoteForwards(capability);
  const trigger = useRef<HTMLButtonElement>(null);
  const handoffTo = useRef<HTMLElement | undefined>(undefined);
  // The control after this one on the rail, or before it when this is the last.
  const nextControl = (): HTMLElement | undefined => {
    const button = trigger.current;
    const controls = Array.from(button?.closest("nav")?.querySelectorAll<HTMLElement>("button:not(:disabled)") ?? []);
    const at = button ? controls.indexOf(button) : -1;
    return controls[at + 1] ?? controls[at - 1];
  };
  // The last forward going takes the button and its popover out of the document, and focus with them. The hand-off
  // below, which goes with them, picks where focus goes while they are still there; focus is moved here, in the commit
  // that removes them, unless something took it meanwhile.
  useLayoutEffect(() => {
    const target = handoffTo.current;
    handoffTo.current = undefined;
    focusHandoffTarget(target);
  });
  if (!capability || forwards.length === 0) return null;
  const focusTriggerIfUnclaimed = () => {
    if (!trigger.current?.isConnected || (document.activeElement && document.activeElement !== document.body)) return;
    trigger.current.focus();
  };
  return (
    <>
      {/* First, so focus in the button or the popover is still found when the last forward takes them away. */}
      <HandoffOnUnmount
        from={() => focusHolder(`#${TRIGGER_ID}, [${PANEL_ATTRIBUTE}]`)}
        ring={false}
        to={() => { handoffTo.current = nextControl(); }}
      />
      <RailPanelEntry
        owner={OWNER}
        label={t("forwards.name")}
        triggerId={TRIGGER_ID}
        triggerLabel={t("rail.forwards", { count: forwards.length })}
        triggerRef={trigger}
        glyph={<Cable aria-hidden="true" className="size-4" />}
        badge={<RailCountBadge count={forwards.length} overflowLabel={t("rail.countBadgeOverflow")} color="success" />}
        onOpenChange={() => {}}
      >
        {({ close, form }) => (
          <>
            {/* A popover that closes with focus inside it hands the focus back to the rail button, once React's
                commit is over so that react-aria sees the move as the keyboard's. */}
            <HandoffOnUnmount from={() => focusHolder(`[${PANEL_ATTRIBUTE}]`)} to={() => queueMicrotask(focusTriggerIfUnclaimed)} />
            <ForwardedPorts
              forwards={forwards}
              daemon={remoteDaemonName() ?? ""}
              // A press opens a popover outside the rail's tab order, so focus is brought to its first row; one a mouse
              // rests on leaves focus where it was.
              focusFirst={form === "press"}
              // The address as the page was given it, which the shell forwards again: the local one would be read as a
              // port of the remote machine.
              onOpen={({ address }) => {
                close();
                void platform.links?.open(address).catch((cause: unknown) => reportExternalLinkFailure(t, address, cause));
              }}
              onStop={({ remotePort }) => void capability.stop(remotePort).catch(alertFailure({ title: t("forwards.stopFailedTitle") }))}
            />
          </>
        )}
      </RailPanelEntry>
    </>
  );
}
