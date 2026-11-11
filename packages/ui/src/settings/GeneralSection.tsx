import React from "react";

import { AppearanceSetting } from "./AppearanceSetting";
import { LanguageSetting } from "./LanguageSetting";
import { MarkdownStyleSetting } from "./MarkdownStyleSetting";
import { SessionOrderSettings } from "./SessionOrderSettings";
import { SettingsGroup } from "./SettingsGroup";

/** The small app-wide settings: the window's appearance, the language and how a Markdown document
 * is styled, then the Sorting group, in which sessions and projects are ordered. */
export function GeneralSection(): React.ReactElement {
  return (
    <>
      {/* Wrapped so the last ungrouped row ends with the gap above the group's heading, not a second line. */}
      <div>
        <AppearanceSetting />
        <LanguageSetting />
        <MarkdownStyleSetting />
      </div>
      <SettingsGroup title="settings.sort.group">
        <SessionOrderSettings />
      </SettingsGroup>
    </>
  );
}
