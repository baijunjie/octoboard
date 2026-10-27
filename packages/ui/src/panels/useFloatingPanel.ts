import { useEffect, useRef, useState } from "react";

import { focusLanding, focusLandingWithoutText, focusTopBar } from "../components/regionFocus";
import { interactionIsTouch } from "../touchInput";
import { OWNER_KINDS, panelId, shownKinds, type OwnerKind, type PanelKind, type PanelStacks } from "./stackState";

type Kinds = Partial<Record<OwnerKind, PanelKind>>;

export interface FloatingPanel {
  /** The panel floating over the agent terminal for `owner`, if one is: never while the window docks
   * the pane, nor while the sidebar's drawer is open. */
  kind: PanelKind | undefined;
  /** Floats `kind` for `owner` kind in place of whichever floated for it, and sends focus into it
   * once it is on screen. */
  open: (owner: OwnerKind, kind: PanelKind) => void;
  /** Closes the panel floating for `owner`, if one is. */
  close: () => void;
  /** Records a panel opened in the docked pane, the one to float first when the window narrows. */
  noteOpened: (owner: OwnerKind, kind: PanelKind) => void;
}

/**
 * The one panel that floats over the agent terminal in a window too narrow to dock the right pane
 * (`floats`, from `useAsideFloats`), per owner kind. It is kept apart from the stacks (`stacks`), so
 * nothing done while the window is narrow rewrites the docked layout the user keeps for a wider
 * one: the stacks only seed it, as the window narrows, with the panel the user opened last in the
 * docked pane (`noteOpened`) if it is still open, else the stack's top one.
 *
 * The sidebar's drawer and a floating panel are never on screen together: opening the drawer closes
 * the panel of every owner kind, the window narrowing while the drawer is open floats none, and no
 * panel shows while it is open.
 *
 * Focus follows the user's own opening and closing: `open` sends it into the panel, waiting for a
 * lazily loaded panel's content unless the user moves it first, and a panel closing, or the window
 * crossing the width that floats panels, with focus left nowhere or in what that made inert (the
 * panel fading away, the terminal now covered) hands it to `focusTerminal`, which lands in a panel
 * still floating over the terminal, or to the top bar when there is no terminal to take it.
 */
export function useFloatingPanel({
  floats,
  owner,
  stacks,
  drawerOpen,
  focusTerminal,
  unavailable = [],
}: {
  floats: boolean;
  owner: OwnerKind | undefined;
  stacks: PanelStacks;
  drawerOpen: boolean;
  focusTerminal: () => void;
  /** Kinds of `owner` with no panel just now (a project's Report panel while the selected session
   * has no pages): one that was floating floats no more, and stays closed when its panel comes back. */
  unavailable?: readonly string[];
}): FloatingPanel {
  const [lastOpened, setLastOpened] = useState<Kinds>({});
  const seeded = (): Kinds =>
    Object.fromEntries((Object.keys(OWNER_KINDS) as OwnerKind[]).map((kind) => {
      const open = shownKinds(stacks[kind], OWNER_KINDS[kind]) as PanelKind[];
      const last = lastOpened[kind];
      return [kind, last !== undefined && open.includes(last) ? last : open[0]];
    }));
  const [kinds, setKinds] = useState<Kinds>(() => (floats && !drawerOpen ? seeded() : {}));
  const [seen, setSeen] = useState({ floats, drawerOpen });
  if (seen.floats !== floats || seen.drawerOpen !== drawerOpen) {
    setSeen({ floats, drawerOpen });
    if (drawerOpen && !seen.drawerOpen) setKinds({});
    else if (floats && !seen.floats) setKinds(drawerOpen ? {} : seeded());
  }
  const floating = floats && owner && !drawerOpen ? kinds[owner] : undefined;
  const kind = floating !== undefined && !unavailable.includes(floating) ? floating : undefined;
  // A kind that went unavailable is no longer the one floating: it is not floated again by itself
  // when it comes back, which would drop focus into a panel the user did not open.
  if (owner && floating !== undefined && kind === undefined) setKinds((current) => ({ ...current, [owner]: undefined }));

  // Each `open` asks once, whether or not the kind it floats was already floating.
  const [focusRequest, setFocusRequest] = useState<{ kind: PanelKind }>();
  useEffect(() => {
    if (!focusRequest) return;
    const card = document.getElementById(panelId(focusRequest.kind));
    // Opened by a tap, it lands where no on-screen keyboard is raised (`interactionIsTouch`).
    const land = interactionIsTouch() ? focusLandingWithoutText : focusLanding;
    if (!card || land(card)) return;
    // Focus the user moves elsewhere meanwhile stays there. Focus that only fell to `<body>`, as it
    // does from the agent terminal the panel has just made inert, was moved by no one.
    const from = document.activeElement;
    const movedByUser = () => {
      const active = document.activeElement;
      return active !== from && active !== document.body && active !== null;
    };
    const observer = new MutationObserver(() => {
      if (movedByUser() || land(card)) observer.disconnect();
    });
    observer.observe(card, { childList: true, subtree: true });
    return () => observer.disconnect();
  }, [focusRequest]);

  const shown = kind !== undefined;
  const layout = useRef({ shown, floats });
  useEffect(() => {
    const before = layout.current;
    layout.current = { shown, floats };
    const closed = before.shown && !shown;
    // Focus is lost on `<body>`, and as good as lost in what was just made inert: the panel fading
    // away as it closes, or the agent terminal a panel now floats over.
    const active = document.activeElement;
    const lost = active === document.body || active === null || active.closest("[inert]") !== null;
    if (!(closed || before.floats !== floats) || !lost) return;
    focusTerminal();
    // With no session there is no terminal to take it, and the terminal leaves `<body>` as it is:
    // the top bar's first control takes it, as it does when a session goes away under focus.
    if (document.activeElement === document.body || document.activeElement === null) focusTopBar();
  }, [shown, floats]);

  return {
    kind,
    open: (ownerKind, next) => {
      setKinds((current) => ({ ...current, [ownerKind]: next }));
      setFocusRequest({ kind: next });
    },
    close: () => {
      if (owner) setKinds((current) => ({ ...current, [owner]: undefined }));
    },
    noteOpened: (ownerKind, opened) => setLastOpened((current) => ({ ...current, [ownerKind]: opened })),
  };
}
