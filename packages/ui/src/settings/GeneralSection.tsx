import React from "react";

import { usePlatform } from "../platform/react";
import { AppearanceSetting } from "./AppearanceSetting";
import { BackgroundRunSetting } from "./BackgroundRunSetting";
import { CloneDirSetting } from "./CloneDirSetting";
import { LanguageSetting } from "./LanguageSetting";
import { MarkdownStyleSetting } from "./MarkdownStyleSetting";

/** The small app-wide settings: the window's appearance, how a Markdown document is styled, the
 * language, the default clone directory and, where the platform offers it, whether closing the window
 * keeps the app running. */
export function GeneralSection(): React.ReactElement {
  const { backgroundRun } = usePlatform();
  return (
    <>
      <AppearanceSetting />
      <MarkdownStyleSetting />
      <LanguageSetting />
      <CloneDirSetting />
      {backgroundRun && <BackgroundRunSetting />}
    </>
  );
}
