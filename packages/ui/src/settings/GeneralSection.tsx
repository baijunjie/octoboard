import React from "react";

import { AppearanceSetting } from "./AppearanceSetting";
import { CloneDirSetting } from "./CloneDirSetting";
import { LanguageSetting } from "./LanguageSetting";

/** The small app-wide settings: the window's appearance and language, and the default clone
 * directory. */
export function GeneralSection(): React.ReactElement {
  return (
    <>
      <AppearanceSetting />
      <LanguageSetting />
      <CloneDirSetting />
    </>
  );
}
