import { InputGroup, TextField } from "@heroui/react";
import { Folder } from "lucide-react";
import React, { useEffect, useRef, useState } from "react";

import { AnchoredTooltip } from "../components/hint/AnchoredTooltip";
import { ValueTooltipText } from "../components/hint/ValueTooltipText";
import { useLatest } from "../components/hooks/useLatest";
import { useDirectoryBrowse } from "../components/dialog/DirectoryPicker";
import { useTrimmedField } from "../components/form/useTrimmedField";
import { Message, useT } from "../i18n/react";
import { isImeKey } from "../input/imeKey";
import { abbreviateHome } from "../format/pathDisplay";
import { useDaemon, useDaemonStore } from "../store";
import { SettingRow } from "./SettingRow";
import { alertAppDataFailure } from "../components/alerts/raiseError";

/** The directory a `git` association clones into when none is named (see `Settings.default_clone_dir`
 * in protocol.generated.ts). The text is sent when the field loses focus, other than to its directory picker,
 * or Enter is pressed, and a directory picked with Browse is sent at once; blanking the field goes
 * back to the built-in default. Escape is the one way out that sends nothing: it closes Settings
 * from under the field, which takes the input away without its ever losing focus, so what was typed
 * is discarded — which is what Escape is taken to mean here. Once a send is answered the field shows
 * the daemon's value again: the normalised form of whatever was typed, written with the daemon
 * host's home directory as `~` (which the daemon expands again when it is sent back), with the full
 * path as the tooltip whenever the two differ. */
export function CloneDirSetting(): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const storedFull = useDaemonStore((s) => s.settings.default_clone_dir);
  const home = useDaemonStore((s) => s.homeDir);
  const stored = abbreviateHome(storedFull, home);
  const [draft, setDraft] = useState(stored);
  const [hovered, setHovered] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  // Follows the daemon's value: another window, or a refused edit, puts the field back to it.
  useEffect(() => setDraft(stored), [stored]);
  // Read after a send is answered, when the closure's `stored` may predate the broadcast.
  const latest = useLatest(stored);

  const draftField = useTrimmedField(draft, setDraft);

  const save = (typed: string) => {
    // An untouched field shows the abbreviated form, which the daemon would only expand back to
    // what it already holds; the full form typed or picked is the same value, shown the usual way.
    if (typed === stored || typed === storedFull) {
      setDraft(stored);
      return;
    }
    void request({ type: "update_settings", default_clone_dir: typed })
      .catch(alertAppDataFailure(t("settings.saveFailedTitle")))
      .finally(() => setDraft(latest.current));
  };

  const directoryBrowse = useDirectoryBrowse(draft, (picked) => {
    setDraft(picked);
    save(picked);
  });

  return (
    <>
      <SettingRow
        fill
        label={t("settings.cloneDir.label")}
        description={<Message id="settings.cloneDir.description.gitUrl" />}
      >
        <div className="flex w-80 items-center gap-2">
          <TextField
            aria-label={t("settings.cloneDir.label")}
            variant="secondary"
            {...draftField}
            onBlur={() => {
              draftField.onBlur();
              // Focus moving into the picker is not leaving the field: what is typed waits for the pick, which
              // replaces it, or for the field's next blur once a cancelled picker has handed focus back.
              if (!directoryBrowse.picking) save(draft);
            }}
            className="min-w-0 flex-1"
          >
            {/* The whole group reads left to right, not just the input: a path is an LTR value, and
                in a right-to-left interface the folder mark would otherwise sit against the far edge
                with the text pushed away from it, the two no longer reading as one control. */}
            <InputGroup dir="ltr" onHoverChange={setHovered}>
              <InputGroup.Prefix>
                <Folder size={14} aria-hidden />
              </InputGroup.Prefix>
              {/* The key handler is on the input rather than on `TextField`, which stops every key it is
                  handed a handler for: Escape would never reach the dialog around the field. */}
              <InputGroup.Input
                ref={input}
                className="font-mono"
                placeholder={t("settings.cloneDir.placeholder")}
                onKeyDown={(event) => event.key === "Enter" && !isImeKey(event.nativeEvent) && save(draft)}
              />
            </InputGroup>
          </TextField>
          {/* The field says the whole path while the pointer is on it, the hover being the group's
              and the tooltip anchored to the input inside it. */}
          {stored !== storedFull && <AnchoredTooltip tooltip={<ValueTooltipText label={input} dir="ltr">{storedFull}</ValueTooltipText>} anchor={input} hovered={hovered} />}
          {/* A mouse press on Browse leaves focus in the field until the picker opens, and a cancelled
              picker hands it back to the field rather than to Browse. */}
          {directoryBrowse.browseButton({ preventFocusOnPress: true })}
        </div>
      </SettingRow>
      {directoryBrowse.picker}
    </>
  );
}
