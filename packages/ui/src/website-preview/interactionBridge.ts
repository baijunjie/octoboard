import { isReportBridgeSource, withReportBridge } from "./reportBridge";

export const PREVIEW_INTERACTION_MESSAGE = "octoboard-preview-interaction";
const REPORT_INTERACTION_MESSAGE = "octoboard-preview-report-interaction";
const INJECTED_MARKER = "data-website-preview-interaction-bridge";

type Modality = "keyboard" | "pointer";

// Serialized into the report's opaque sandbox, so this function must have no module dependencies.
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

export function withReportInteractionBridge(srcdoc: string): string {
  const source = `(${installInteractionRelay.toString()})(window, function(modality) { parent.postMessage({ type: "${REPORT_INTERACTION_MESSAGE}", modality: modality }, "*"); });`;
  return withReportBridge(srcdoc, INJECTED_MARKER, source);
}

export function installPreviewInteractionBridge(win: Window = window): () => void {
  if (win.parent === win) return () => {};
  const send = (modality: Modality) =>
    win.parent.postMessage({ type: PREVIEW_INTERACTION_MESSAGE, modality }, "*");
  const removeRelay = installInteractionRelay(win, send);
  const onMessage = (event: MessageEvent) => {
    const data = event.data as { type?: unknown; modality?: unknown } | null;
    if (
      data?.type !== REPORT_INTERACTION_MESSAGE ||
      (data.modality !== "keyboard" && data.modality !== "pointer")
    )
      return;
    if (isReportBridgeSource(win.document, event.source, INJECTED_MARKER))
      send(data.modality);
  };
  win.addEventListener("message", onMessage);
  return () => {
    removeRelay();
    win.removeEventListener("message", onMessage);
  };
}
