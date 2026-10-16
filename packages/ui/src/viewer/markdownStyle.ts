import { createPersistedPreference } from "../persistedPreference";
import { PREFERENCE_KEYS } from "../preferenceKeys";

export const MARKDOWN_STYLES = ["tailwind", "github", "newsprint", "latex"] as const;

export type MarkdownStyle = (typeof MARKDOWN_STYLES)[number];

/** The style a stored text names; Tailwind, the original look, for no text or one it does not know. */
export function parseMarkdownStyle(raw: string | null): MarkdownStyle {
  return MARKDOWN_STYLES.find((style) => style === raw) ?? "tailwind";
}

/** How a rendered Markdown document is styled: the user's choice in Settings, kept in this client's
 * `localStorage` and applied to every open document at once. */
export const markdownStyle = createPersistedPreference<MarkdownStyle>(
  PREFERENCE_KEYS.markdownStyle,
  parseMarkdownStyle,
  (value) => value,
);
