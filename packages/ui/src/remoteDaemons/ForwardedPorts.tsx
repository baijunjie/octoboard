import { SquareArrowOutUpRight } from "lucide-react";
import React, { useEffect, useLayoutEffect, useRef } from "react";

import { useHandoffAfterRemoval } from "../components/focus/HandoffOnUnmount";
import { TitledControl } from "../components/hint/TitledControl";
import { ValueTooltipText } from "../components/hint/ValueTooltipText";
import { useListMotion } from "../components/motion/useListMotion";
import { FadedScroller } from "../components/scroll/scrollFade";
import { FadeOverflow } from "../components/text/FadeOverflow";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import type { RemoteForward } from "../platform";
import { Button } from "../primitives";

/** The attribute that marks the panel's root, for finding what holds focus. */
export const PANEL_ATTRIBUTE = "data-rail-forwards-panel";

/** The host and port of the address on this computer, as the row shows it (`localhost:3000`). */
function shownAddress(localAddress: string): string {
  try {
    return new URL(localAddress).host;
  } catch {
    return localAddress;
  }
}

/** The forwarded ports popover's contents: what the list is, then a row for each forward: the address on this computer,
 * the remote port and the daemon beneath it, pressed to open the address again, and Stop. A forward stopped from the
 * keyboard takes its focused Stop with it, so focus goes on to the next row's Stop, or the previous one's for the last
 * row (the panel's own handoff takes it when none is left). */
export function ForwardedPorts({ forwards, daemon, focusFirst, onOpen, onStop }: {
  forwards: RemoteForward[];
  /** The remote daemon's name, as its window shows it. */
  daemon: string;
  focusFirst: boolean;
  onOpen: (forward: RemoteForward) => void;
  onStop: (forward: RemoteForward) => void;
}): React.ReactElement {
  const t = useT();
  const root = useRef<HTMLDivElement>(null);
  const listRef = useListMotion<HTMLUListElement>({ descendants: true, fitHeight: true });
  useEffect(() => {
    if (focusFirst) root.current?.querySelector<HTMLElement>("ul button")?.focus();
  }, []);
  // The forwards of the previous render, to find which one a render took away.
  const shown = useRef(forwards);
  // A forward stopped from the keyboard takes its focused Stop with it, so focus goes on to the next row's Stop, or the
  // previous one's for the last row.
  const handoff = useHandoffAfterRemoval(() => {
    const before = shown.current;
    const kept = (forward: RemoteForward) => forwards.some((other) => other.remotePort === forward.remotePort);
    const removed = before.findIndex((forward) => !kept(forward));
    const neighbour = before.slice(removed + 1).find(kept) ?? before.slice(0, removed).reverse().find(kept);
    return neighbour && root.current?.querySelector<HTMLElement>(`li[data-list-motion="${neighbour.remotePort}"] [data-forward-stop]`);
  });
  useLayoutEffect(() => {
    shown.current = forwards;
  });
  const name = isolate(daemon);
  return (
    <FadedScroller ref={root} {...{ [PANEL_ATTRIBUTE]: "" }} className="flex max-h-[70vh] flex-col gap-1 p-2">
      <div className="flex flex-col gap-1 px-2 py-1.5">
        <h2 className="text-sm font-semibold">{t("forwards.name")}</h2>
        <p className="text-xs text-app-muted">{t("forwards.description", { name })}</p>
      </div>
      <ul ref={listRef} className="relative flex min-w-0 flex-col gap-1">
        {forwards.map((forward) => (
          <ForwardRow key={forward.remotePort} forward={forward} name={name} handoff={handoff} onOpen={onOpen} onStop={onStop} />
        ))}
      </ul>
    </FadedScroller>
  );
}

/** A forward's row: pressed to open its address again, with the address in a tooltip, and Stop. `handoff` is the panel's
 * handoff of a Stop that goes with its row, placed first so that focus on the Stop is still found as the row goes. */
function ForwardRow({ forward, name, handoff, onOpen, onStop }: {
  forward: RemoteForward;
  name: string;
  handoff: (from: string) => React.ReactElement;
  onOpen: (forward: RemoteForward) => void;
  onStop: (forward: RemoteForward) => void;
}): React.ReactElement {
  const t = useT();
  const address = useRef<HTMLElement>(null);
  return (
    <li data-list-motion={String(forward.remotePort)} className="flex min-w-0 items-center gap-1">
      {handoff(`li[data-list-motion="${forward.remotePort}"] [data-forward-stop]`)}
      <TitledControl title={<ValueTooltipText label={address} dir="ltr">{forward.address}</ValueTooltipText>} placement="end">
        <Button variant="ghost" size="sm" className="h-auto min-h-0 min-w-0 flex-1 justify-start gap-2 rounded-lg px-2 py-1.5 font-normal"
          label={t("forwards.open", { address: shownAddress(forward.localAddress), port: String(forward.remotePort), name })}
          // The tooltip repeats the address the name already says.
          describedBy=""
          onPress={() => onOpen(forward)}>
          <span className="flex min-w-0 flex-1 flex-col text-start">
            <FadeOverflow ref={address} as="span" dir="ltr" className="block font-semibold">{shownAddress(forward.localAddress)}</FadeOverflow>
            <FadeOverflow as="span" dir="auto" className="block text-xs text-app-muted">{t("forwards.remote", { port: String(forward.remotePort), name })}</FadeOverflow>
          </span>
          <SquareArrowOutUpRight aria-hidden="true" className="-me-0.5 size-3.5 shrink-0 text-app-muted" />
        </Button>
      </TitledControl>
      <Button size="xs" variant="danger-soft" className="shrink-0" label={t("forwards.stopNamed", { port: String(forward.remotePort) })} focusOnPress={false} data-forward-stop="" onPress={() => onStop(forward)}>
        {t("forwards.stop")}
      </Button>
    </li>
  );
}
