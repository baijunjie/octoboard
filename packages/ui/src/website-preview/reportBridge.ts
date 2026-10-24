export function withReportBridge(
  srcdoc: string,
  marker: string,
  source: string,
): string {
  if (
    !srcdoc.includes("data-website-preview-report") ||
    !srcdoc.includes("window.OCTOBOARD_PAGE") ||
    srcdoc.includes(marker)
  )
    return srcdoc;
  const nonce = /<script nonce="([a-f0-9]+)">/.exec(srcdoc)?.[1];
  if (!nonce) return srcdoc;
  return `${srcdoc}<script nonce="${nonce}" ${marker}>${source}</script>`;
}

export function isReportBridgeSource(
  doc: Document,
  source: MessageEventSource | null,
  marker: string,
): boolean {
  return [
    ...doc.querySelectorAll<HTMLIFrameElement>(".website-preview-app iframe[srcdoc]"),
  ].some(
    (frame) => frame.srcdoc.includes(marker) && source === frame.contentWindow,
  );
}
