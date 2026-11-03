import type { ProviderMark } from "./types";

/** The mark of a provider that has no presentation module: a rounded square with a round hole, neutral and in the text
 * colour. */
export const GENERIC_MARK: ProviderMark = {
  fill: "currentColor",
  d: "M5 3h14a2 2 0 012 2v14a2 2 0 01-2 2H5a2 2 0 01-2-2V5a2 2 0 012-2zm7 4.5a4.5 4.5 0 100 9 4.5 4.5 0 000-9z",
};
