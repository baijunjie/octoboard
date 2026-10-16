import React from "react";

import { OptionSelect } from "../dialogs/OptionSelect";
import { useT } from "../i18n/react";
import { MARKDOWN_STYLES, markdownStyle, type MarkdownStyle } from "../viewer/markdownStyle";
import { SettingRow } from "./SettingRow";

// The styles are named for what they imitate, so the names are proper names and the same in every language.
const NAMES: Record<MarkdownStyle, string> = { tailwind: "Tailwind", github: "GitHub", newsprint: "Newsprint", latex: "LaTeX" };

const OPTIONS = MARKDOWN_STYLES.map((style) => ({ value: style, label: NAMES[style], lang: "en" }));

/** How a rendered Markdown document is styled in the file viewer. */
export function MarkdownStyleSetting(): React.ReactElement {
  const t = useT();
  const style = markdownStyle.useValue();
  return (
    <SettingRow label={t("settings.markdownStyle.label")} description={t("settings.markdownStyle.description")}>
      <OptionSelect inline label={t("settings.markdownStyle.label")} options={OPTIONS} value={style} onChange={markdownStyle.set} />
    </SettingRow>
  );
}
