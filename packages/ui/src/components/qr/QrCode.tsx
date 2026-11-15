import React from "react";

/** The quiet zone a QR code needs around it to be scanned, in modules: the standard's minimum of four. */
export const QR_QUIET_ZONE = 4;

/** The path that draws the dark modules of `rows`, one subpath per run of dark modules in a row, offset by the quiet
 * zone. */
export function qrPath(rows: string[]): string {
  const parts: string[] = [];
  rows.forEach((row, y) => {
    for (const run of row.matchAll(/1+/g)) {
      parts.push(`M${run.index + QR_QUIET_ZONE} ${y + QR_QUIET_ZONE}h${run[0].length}v1h-${run[0].length}z`);
    }
  });
  return parts.join("");
}

/** A QR code drawn as one path of its dark modules on a white square with its quiet zone. `rows` is one string per row
 * from the top, `"1"` a dark module and `"0"` a light one, each character from the left, with no quiet zone. The code
 * is black on white in a dark theme too, since a phone's camera reads it that way; the colours are the code's own, not
 * the theme's. `label` names it for a screen reader, which cannot read it. `size` is its side in CSS pixels. */
export function QrCode({ rows, label, size = 176 }: { rows: string[]; label: string; size?: number }): React.ReactElement {
  const modules = Math.max(0, ...rows.map((row) => row.length));
  const side = modules + 2 * QR_QUIET_ZONE;
  return (
    <svg
      role="img"
      aria-label={label}
      viewBox={`0 0 ${side} ${side}`}
      width={size}
      height={size}
      shapeRendering="crispEdges"
      className="shrink-0 rounded-lg"
    >
      <rect width={side} height={side} fill="#fff" />
      <path d={qrPath(rows)} fill="#000" />
    </svg>
  );
}
