import React from "react";

import { OptionSelect } from "../components/form/OptionSelect";
import { useT } from "../i18n/react";
import { MARKDOWN_STYLE_NAMES, MARKDOWN_STYLES, markdownStyle } from "../viewer/markdown/markdownStyle";
import { SettingRow } from "./SettingRow";

const OPTIONS = MARKDOWN_STYLES.map((style) => ({ value: style, label: MARKDOWN_STYLE_NAMES[style], lang: "en" }));

/** How a rendered Markdown document is styled in the file viewer. */
export function MarkdownStyleSetting(): React.ReactElement {
  const t = useT();
  const style = markdownStyle.useValue();
  return (
    <SettingRow fill label={t("settings.markdownStyle.label")} description={t("settings.markdownStyle.description")}>
      <OptionSelect inline label={t("settings.markdownStyle.label")} options={OPTIONS} value={style} onChange={markdownStyle.set} />
    </SettingRow>
  );
}
