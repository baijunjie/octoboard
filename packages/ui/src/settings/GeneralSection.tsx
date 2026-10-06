import React from "react";

import { AppearanceSetting } from "./AppearanceSetting";
import { LanguageSetting } from "./LanguageSetting";

/** The settings that shape the whole window rather than one feature: its appearance and its language. */
export function GeneralSection(): React.ReactElement {
  return (
    <>
      <AppearanceSetting />
      <LanguageSetting />
    </>
  );
}
