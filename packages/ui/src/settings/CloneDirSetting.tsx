import { Button, Input, TextField } from "@heroui/react";
import React, { useEffect, useRef, useState } from "react";

import { DirectoryPicker } from "../dialogs/DirectoryPicker";
import { useT } from "../i18n/react";
import { useDaemon, useDaemonStore } from "../store";
import { SettingRow } from "./SettingRow";

/** The directory a `git` association clones into when none is named (see `Settings.default_clone_dir`
 * in protocol.ts). The text is sent when the field loses focus or Enter is pressed, and a directory
 * picked with Browse is sent at once; blanking the field goes back to the built-in default. Once a
 * send is answered the field shows the daemon's value again, which is the normalised, expanded form
 * of whatever was typed. */
export function CloneDirSetting(): React.ReactElement {
  const t = useT();
  const { request, toastError } = useDaemon();
  const stored = useDaemonStore((s) => s.settings.default_clone_dir);
  const [draft, setDraft] = useState(stored);
  const [picking, setPicking] = useState(false);
  // Follows the daemon's value: another window, or a refused edit, puts the field back to it.
  useEffect(() => setDraft(stored), [stored]);
  // Read after a send is answered, when the closure's `stored` may predate the broadcast.
  const latest = useRef(stored);
  latest.current = stored;

  const save = (value: string) => {
    if (value.trim() === stored) return;
    void request({ type: "update_settings", default_clone_dir: value })
      .catch((err) => toastError((err as Error).message))
      .finally(() => setDraft(latest.current));
  };

  return (
    <>
      <SettingRow label={t("settings.cloneDir.label")} description={t("settings.cloneDir.description")}>
        <div className="flex w-80 items-center gap-2">
          <TextField
            aria-label={t("settings.cloneDir.label")}
            variant="secondary"
            value={draft}
            onChange={setDraft}
            onBlur={() => save(draft)}
            onKeyDown={(event) => event.key === "Enter" && !event.nativeEvent.isComposing && event.nativeEvent.keyCode !== 229 && save(draft)}
            className="min-w-0 flex-1"
          >
            <Input dir="ltr" placeholder={t("settings.cloneDir.placeholder")} />
          </TextField>
          {/* Not taking focus on a mouse press keeps the field from losing it, and so from sending
              what is typed, just to open the picker. */}
          <div onMouseDown={(event) => event.preventDefault()}>
            <Button type="button" variant="secondary" onPress={() => setPicking(true)}>
              {t("common.browse")}
            </Button>
          </div>
        </div>
      </SettingRow>
      {picking && (
        <DirectoryPicker
          title={t("dialog.chooseDirectory")}
          initialPath={stored || "~"}
          onPick={(picked) => {
            setPicking(false);
            setDraft(picked);
            save(picked);
          }}
          onClose={() => setPicking(false)}
        />
      )}
    </>
  );
}
