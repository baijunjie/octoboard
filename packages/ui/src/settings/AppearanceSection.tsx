import React from "react";

import { ThemeSetting } from "./ThemeSetting";
import { MarkdownStyleSetting } from "./MarkdownStyleSetting";
import { TerminalFontSizeSetting } from "./TerminalFontSizeSetting";

/** How the app looks: the light, dark or system theme, how a Markdown document is styled and the terminal font size. */
export function AppearanceSection(): React.ReactElement {
  return (
    <>
      <ThemeSetting />
      <MarkdownStyleSetting />
      <TerminalFontSizeSetting />
    </>
  );
}
