import React from "react";

import { usePlatform } from "../platform/react";
import { AppearanceSetting } from "./AppearanceSetting";
import { BackgroundRunSetting } from "./BackgroundRunSetting";
import { DaemonSwitchSetting } from "./DaemonSwitchSetting";
import { LanguageSetting } from "./LanguageSetting";
import { MarkdownStyleSetting } from "./MarkdownStyleSetting";

/** The small app-wide settings: the window's appearance, how a Markdown document is styled, the
 * language, whether the sidebar is ordered by status, whether archived sessions whose conversation
 * is gone are deleted and, where the platform offers it, whether closing the window keeps the app
 * running. */
export function GeneralSection(): React.ReactElement {
  const { backgroundRun } = usePlatform();
  return (
    <>
      <AppearanceSetting />
      <LanguageSetting />
      <MarkdownStyleSetting />
      <DaemonSwitchSetting field="sidebar_order_by_status" />
      <DaemonSwitchSetting field="auto_delete_unresumable_archives" />
      {backgroundRun && <BackgroundRunSetting />}
    </>
  );
}
