import { Folder } from "lucide-react";
import React, { useEffect, useRef, useState } from "react";

import { ChangedElsewhereAnnouncer } from "../components/announce/ChangedElsewhereAnnouncer";
import { AnchoredTooltip } from "../components/hint/AnchoredTooltip";
import { ValueTooltipText } from "../components/hint/ValueTooltipText";
import { useLatest, useStableHandler } from "../components/hooks/useLatest";
import { useDirectoryBrowse } from "../components/dialog/DirectoryPicker";
import { useTrimmedField } from "../components/form/useTrimmedField";
import { Message, useT } from "../i18n/react";
import { Input, TextField } from "../primitives";
import { isImeKey } from "../input/imeKey";
import { display } from "../format/hostPath";
import { CodedError } from "../errors/codedError";
import { baseVersions, reachedConflict, writeConflict } from "../errors/writeConflict";
import { useDaemon, useDaemonStore } from "../store";
import { whenState } from "../store/whenState";
import { useHostPaths } from "../components/hooks/useHostPaths";
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
 * path as the tooltip whenever the two differ. The send is based on the setting's version in the store, so another
 * client's change is refused as a conflict and the field then shows that change, once the store holds it. One send is
 * on the wire at a time, as in the agent tags: a value saved meanwhile waits, and is sent at the reply on the store's
 * version then, only while the store holds a value this window's sends explain (the stored value a send went over, or a
 * value sent); with another client's value there it would overwrite it, so it is dropped, the conflict reported and the
 * stored value shown. A send refused as a conflict while the store is behind this window's own earlier send is sent
 * again once the store holds the versions the refusal names, if the stored value is explained, and is another
 * client's otherwise. A waiting value is still sent after the field has gone, and a failure then is reported. A
 * change of the value made elsewhere replaces what the field shows, and is announced to assistive technology when
 * it replaces what the user had typed.
 *
 * A value another client wrote that equals one this window sent, or one a send went over, is not told apart
 * from this window's own, and a value the daemon normalises differently from how it is shown is taken as another
 * client's. */
export function CloneDirSetting(): React.ReactElement {
  const t = useT();
  const { request, store } = useDaemon();
  const storedFull = useDaemonStore((s) => s.settings.default_clone_dir);
  const paths = useHostPaths();
  const stored = display(storedFull, paths);
  const [draft, setDraft] = useState(stored);
  const [hovered, setHovered] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const latestDraft = useLatest(draft);
  const latestPaths = useLatest(paths);
  // The values this window's sends explain: the stored value each send went over and each value sent, kept while the
  // broadcast of a send answered before it may still arrive, so that it is not taken for another client's. Emptied once
  // the store holds the last value sent with no run active, and by a change that is another client's.
  const explained = useRef<string[]>([]);
  // The value of the send that is on the wire, or the last one sent.
  const sending = useRef<string>(undefined);
  // A send is on the wire, or waiting for the store after a conflict.
  const running = useRef(false);
  // The value saved while a send was out, which goes at the reply: the latest, since it is what the user last meant.
  const waiting = useRef<string>(undefined);
  // What the store showed before its latest change, which a draft is judged against: one that differs was typed.
  const shown = useRef(stored);
  const gone = useRef(false);
  const [replaced, setReplaced] = useState(0);
  useEffect(() => {
    gone.current = false;
    return () => {
      gone.current = true;
    };
  }, []);

  /** Whether the store holds a value this window's sends explain. */
  const explainsStored = () => {
    const full = store.getState().settings.default_clone_dir;
    const shownForm = display(full, latestPaths.current);
    return explained.current.some((value) => value === full || value === shownForm);
  };
  // Empties `explained` once the store holds the last value sent and no run is active: the broadcast of the last send
  // has arrived, so nothing is left to explain.
  const settleExplained = () => {
    if (running.current || sending.current === undefined) return;
    const full = store.getState().settings.default_clone_dir;
    if (sending.current === full || sending.current === display(full, latestPaths.current)) explained.current = [];
  };
  // Sends `first`, then the value that waited, one after another, each on the version the store has when it goes. It
  // reads the store itself, as it goes on after the field has gone.
  const drive = useStableHandler(async (first: string) => {
    const report = alertAppDataFailure(t("settings.saveFailedTitle"));
    // Ends the run, and the field shows the stored value again.
    const finish = () => {
      running.current = false;
      waiting.current = undefined;
      settleExplained();
      if (!gone.current) setDraft(display(store.getState().settings.default_clone_dir, latestPaths.current));
    };
    let value = first;
    while (true) {
      const outcome = await request({
        type: "update_settings",
        default_clone_dir: value,
        base_versions: baseVersions(store.getState().settings.versions, ["default_clone_dir"]),
      }).then(() => ({ failed: false as const }), (error: unknown) => ({ failed: true as const, error }));
      const named = outcome.failed ? writeConflict(outcome.error) : undefined;
      if (outcome.failed && named) {
        await whenState(store, (state) => reachedConflict(named, state.settings.versions));
        if (explainsStored()) {
          // The store was behind this window's own earlier send: the value goes again on the version it has now.
          value = waiting.current ?? value;
          sending.current = value;
          waiting.current = undefined;
          if (!explained.current.includes(value)) explained.current.push(value);
          continue;
        }
        // Another client's change, which is shown now; what was typed, and what waited, is dropped.
        finish();
        return report(outcome.error);
      }
      if (outcome.failed) report(outcome.error);
      const next = waiting.current;
      waiting.current = undefined;
      if (next === undefined) return finish();
      if (!explainsStored()) {
        // The store holds a value none of this window's sends explains: another client's, which `next` would overwrite.
        finish();
        // The code has catalogued wording in every language, so the message here is never shown.
        return outcome.failed ? undefined : report(new CodedError("changed elsewhere", "write_conflict", { fields: "default_clone_dir", versions: "{}" }));
      }
      value = next;
      sending.current = value;
      explained.current.push(next);
    }
  });
  const send = (typed: string) => {
    running.current = true;
    sending.current = typed;
    // A broadcast of the user's own earlier save may still be on its way, so what explains it stays; the value stored now
    // is the one this send goes over, so it is this window's to explain too.
    explained.current = [...new Set([...explained.current, store.getState().settings.default_clone_dir, typed])];
    void drive(typed);
  };
  // Follows the daemon's value when it changes: another window puts the field to it, announced when it replaces what the
  // user had typed. The change of this window's own send leaves what is typed since alone, and a version that moves over
  // the same value never gets here.
  useEffect(() => {
    const previous = shown.current;
    shown.current = stored;
    // What waited stays in the field, sent at the reply or dropped there.
    if (waiting.current !== undefined) return;
    const edited = latestDraft.current !== previous;
    const own = explainsStored();
    if (own && edited) {
      settleExplained();
      return;
    }
    setDraft(stored);
    if (own) {
      settleExplained();
      return;
    }
    // A change under a send that is out is answered by that send's own result, which speaks for it.
    if (running.current) return;
    explained.current = [];
    if (edited) setReplaced((count) => count + 1);
  }, [stored]);

  const draftField = useTrimmedField(draft, setDraft);

  const save = (typed: string) => {
    if (running.current) {
      waiting.current = typed === sending.current ? undefined : typed;
      return;
    }
    // An untouched field shows the abbreviated form, which the daemon would only expand back to
    // what it already holds; the full form typed or picked is the same value, shown the usual way.
    if (typed === stored || typed === storedFull) {
      setDraft(stored);
      return;
    }
    send(typed);
  };

  const directoryBrowse = useDirectoryBrowse(draft, (picked) => {
    setDraft(picked);
    save(picked);
  });

  return (
    <>
      <SettingRow
        fill
        searchId="clone-dir"
        label={t("settings.cloneDir.label")}
        description={<Message id="settings.cloneDir.description.gitUrl" />}
      >
        <div className="flex w-80 items-center gap-2">
          <TextField
            label={t("settings.cloneDir.label")}
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
            {/* The key handler is on the input rather than on `TextField`, which stops every key it is handed a
                handler for: Escape would never reach the dialog around the field. */}
            <Input
              dir="ltr"
              onHoverChange={setHovered}
              start={<Folder size={14} aria-hidden />}
              ref={input}
              inputClassName="font-mono"
              placeholder={t("settings.cloneDir.placeholder")}
              onKeyDown={(event) => event.key === "Enter" && !isImeKey(event.nativeEvent) && save(draft)}
            />
          </TextField>
          {/* The field says the whole path while the pointer is on it, the hover being the group's
              and the tooltip anchored to the input inside it. */}
          {stored !== storedFull && <AnchoredTooltip tooltip={<ValueTooltipText label={input} dir="ltr">{storedFull}</ValueTooltipText>} anchor={input} hovered={hovered} />}
          {/* A mouse press on Browse leaves focus in the field until the picker opens, and a cancelled
              picker hands it back to the field rather than to Browse. */}
          {directoryBrowse.browseButton({ focusOnPress: false })}
        </div>
      </SettingRow>
      {directoryBrowse.picker}
      <ChangedElsewhereAnnouncer count={replaced} labels={[t("settings.cloneDir.label")]} />
    </>
  );
}
