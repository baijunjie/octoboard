import { Button, InputGroup, TextField } from "@heroui/react";
import { Folder } from "lucide-react";
import React, { useEffect, useRef, useState } from "react";

import { DirectoryPicker } from "../dialogs/DirectoryPicker";
import { useTrimmedField } from "../dialogs/useTrimmedField";
import { useT } from "../i18n/react";
import { isImeKey } from "../imeKey";
import { abbreviateHome } from "../pathDisplay";
import { useDaemon, useDaemonStore } from "../store";
import { SettingRow } from "./SettingRow";

/** The directory a `git` association clones into when none is named (see `Settings.default_clone_dir`
 * in protocol.ts). The text is sent when the field loses focus or Enter is pressed, and a directory
 * picked with Browse is sent at once; blanking the field goes back to the built-in default. Once a
 * send is answered the field shows the daemon's value again: the normalised form of whatever was
 * typed, written with the daemon host's home directory as `~` (which the daemon expands again when
 * it is sent back), with the full path as the tooltip whenever the two differ. */
export function CloneDirSetting(): React.ReactElement {
  const t = useT();
  const { request, toastError } = useDaemon();
  const storedFull = useDaemonStore((s) => s.settings.default_clone_dir);
  const home = useDaemonStore((s) => s.homeDir);
  const stored = abbreviateHome(storedFull, home);
  const [draft, setDraft] = useState(stored);
  const [picking, setPicking] = useState(false);
  // Follows the daemon's value: another window, or a refused edit, puts the field back to it.
  useEffect(() => setDraft(stored), [stored]);
  // Read after a send is answered, when the closure's `stored` may predate the broadcast.
  const latest = useRef(stored);
  latest.current = stored;

  const draftField = useTrimmedField(draft, setDraft);

  const save = (typed: string) => {
    // An untouched field shows the abbreviated form, which the daemon would only expand back to
    // what it already holds; the full form typed or picked is the same value, shown the usual way.
    if (typed === stored || typed === storedFull) {
      setDraft(stored);
      return;
    }
    void request({ type: "update_settings", default_clone_dir: typed })
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
            {...draftField}
            onBlur={() => {
              draftField.onBlur();
              save(draft);
            }}
            onKeyDown={(event) => event.key === "Enter" && !isImeKey(event.nativeEvent) && save(draft)}
            className="min-w-0 flex-1"
          >
            <InputGroup title={stored === storedFull ? undefined : storedFull}>
              <InputGroup.Prefix>
                <Folder size={14} aria-hidden />
              </InputGroup.Prefix>
              <InputGroup.Input dir="ltr" className="font-mono" placeholder={t("settings.cloneDir.placeholder")} />
            </InputGroup>
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
          initialPath={storedFull || "~"}
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
