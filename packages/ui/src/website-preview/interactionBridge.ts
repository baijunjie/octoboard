import { PREVIEW_MESSAGE, postPreviewMessage, type Modality } from "./messages";

/** Reports the last kind of input in `win` (a key, or a pointer press) to `send`. */
export function installInteractionRelay(
  win: Window,
  send: (modality: Modality) => void,
): () => void {
  const onKeyDown = (event: KeyboardEvent) => {
    if (!["Alt", "Control", "Meta", "Shift"].includes(event.key)) send("keyboard");
  };
  const onPointerDown = () => send("pointer");
  // Observe before a terminal or a shadow-root control consumes the event; leave its behavior intact.
  win.addEventListener("keydown", onKeyDown, true);
  win.addEventListener("pointerdown", onPointerDown, true);
  return () => {
    win.removeEventListener("keydown", onKeyDown, true);
    win.removeEventListener("pointerdown", onPointerDown, true);
  };
}

export function installPreviewInteractionBridge(win: Window = window): () => void {
  if (win.parent === win) return () => {};
  const send = (modality: Modality) =>
    postPreviewMessage(win.parent, { type: PREVIEW_MESSAGE.interaction, modality });
  return installInteractionRelay(win, send);
}
