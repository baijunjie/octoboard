import type React from "react";

import { providerColour } from "../providers/registry";

/** The class of a quota bar drawn in its provider's colour, the one `QuotaMeter.css` paints `--meter-fill` under. */
export const PROVIDER_FILL_CLASS = "quota-meter--provider";

/** A provider's colour in both appearances, as the two custom properties `.quota-meter--provider` in `QuotaMeter.css`
 * paints with, the second one in the dark appearance; `undefined` for a provider without a colour, whose bars draw in
 * the foreground. */
export function providerFillVars(provider: string): React.CSSProperties | undefined {
  const colour = providerColour(provider);
  return colour && ({ "--provider-fill-light": colour.light, "--provider-fill-dark": colour.dark } as React.CSSProperties);
}
