import React from "react";

import { AppearanceSetting } from "./AppearanceSetting";
import { DaemonSwitchSetting } from "./DaemonSwitchSetting";
import { LanguageSetting } from "./LanguageSetting";
import { MarkdownStyleSetting } from "./MarkdownStyleSetting";

/** The small app-wide settings: the window's appearance, how a Markdown document is styled, the
 * language and whether the sidebar is ordered by status. */
export function GeneralSection(): React.ReactElement {
  return (
    <>
      <AppearanceSetting />
      <LanguageSetting />
      <MarkdownStyleSetting />
      <DaemonSwitchSetting field="sidebar_order_by_status" />
    </>
  );
}
