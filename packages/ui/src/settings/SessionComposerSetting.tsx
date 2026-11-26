import React from "react";

import { sessionComposerEnabled } from "../composer/composerSetting";
import { useT } from "../i18n/react";
import { SwitchSettingRow } from "./SettingRow";

/** Whether the message composer shows below the selected session's terminal: this client's own choice
 * (`sessionComposerEnabled`), not the daemon's, so it changes nothing in another device's windows. Each window ends its
 * own edits as it goes off (`useEndEditsWhenOff`). */
export function SessionComposerSetting(): React.ReactElement {
  const t = useT();
  const enabled = sessionComposerEnabled.useValue();
  return (
    <SwitchSettingRow
      searchId="session-composer"
      label={t("settings.composer.label")}
      description={t("settings.composer.description")}
      isSelected={enabled}
      onChange={(value) => sessionComposerEnabled.set(value)}
    />
  );
}
