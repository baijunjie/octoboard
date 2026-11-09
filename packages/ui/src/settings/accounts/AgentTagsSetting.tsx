import React, { useLayoutEffect, useRef, useState } from "react";

import { listedAgents, servedAgents } from "../../agents/agents";
import { providerLabel } from "../../agents/providers";
import { alertAppDataFailure } from "../../components/alerts/raiseError";
import { TagsInput } from "../../components/form/TagsInput";
import { tagVocabulary } from "../../components/form/tagVocabulary";
import { useStableHandler } from "../../components/hooks/useLatest";
import { HelpTip } from "../../components/hint/HelpTip";
import { CodedError } from "../../errors/codedError";
import { baseVersions, reachedConflict, writeConflict } from "../../errors/writeConflict";
import { isolate } from "../../i18n/isolate";
import { useT } from "../../i18n/react";
import type { Agent } from "../../protocol";
import { useDaemon, useDaemonStore, useProviders } from "../../store";
import { whenState } from "../../store/whenState";
import { sameValue } from "../../values/sameValue";
import { SettingRow } from "../SettingRow";

/** An agent's Tags row, the first of its settings dialog: free-form labels saying what the agent is good for,
 * which apply to every account of the agent (see "Agent tags" in
 * `docs/product/per-agent-settings.md`). The tags offered are those the added agents carry (every
 * agent while the list is undecided); there is no registry of them. Saved on every change as the
 * whole list, which the daemon normalises, based on the agent's tags version in the store
 * (`agent_tags.<agent>`), so another client's change to them is refused as a conflict rather than
 * overwritten. One save is on the wire at a time: an edit made meanwhile waits, and the one sent
 * after it carries every edit since. A save the daemon refuses as a conflict while the store is behind our own
 * earlier save (the reply came before its broadcast) is not another client's: once the store holds the versions the
 * conflict names, a list our saves explain is sent again on the store's version, and any other list is a concurrent
 * writer's, which is reported and shown. The field shows the list last asked for while any save
 * is outstanding, so tags added in quick succession build on one another and no broadcast of an
 * earlier save shows in between. Once none is outstanding it goes on showing it, since the reply
 * may come before the broadcast of the change, until the store's list equals it or is a value none
 * of our saves explains (the stored list the first save started from, or a list we sent, is
 * explained; anything else is a concurrent writer's or a different normalisation), which is then
 * shown and built on by the next edit. A foreign write that equals a value we sent, or the list
 * the first save started from, is not told apart from ours. When a save fails it is reported, and
 * an edit that waited is still sent, since it carries the whole list; with none waiting the stored
 * tags show again. An edit that waited is sent only while the store's list is one our saves explain: with
 * another client's list there, it would overwrite it, so it is dropped, the conflict reported and the stored list
 * shown. The run goes on after the field has gone, reading the store itself: an edit that waited is still sent, and a
 * conflict or failure then is reported through the error alert. */
export function AgentTagsSetting({ agent }: { agent: Agent }): React.ReactElement {
  const t = useT();
  const { request, store } = useDaemon();
  const added = useDaemonStore((s) => s.settings.added_agents);
  const providers = useProviders();
  const allTags = useDaemonStore((s) => s.settings.agent_tags);
  const [asked, setAsked] = useState<string[]>();
  // The lists our outstanding and settled saves explain: the stored list the first one started
  // from, then each one sent. Empty exactly when `asked` is unset.
  const [expected, setExpected] = useState<string[][]>([]);
  // A save is on the wire, waiting for the store after a conflict, or waiting its turn, for the render.
  const [saving, setSaving] = useState(false);
  // The same, for `change` to read at once: two edits in one event see no render between them.
  const inFlight = useRef(false);
  // The edit that waits for the save on the wire to settle: the latest, which carries the earlier ones.
  const waiting = useRef<string[] | undefined>(undefined);
  // `expected` for `drive` to read, which goes on after the field has gone and sees no render; `change` writes it too, as
  // two edits in one event see no render between them.
  const expectedNow = useRef(expected);
  useLayoutEffect(() => {
    expectedNow.current = expected;
  });
  const stored = allTags[agent] ?? [];
  const explained = expected.some((list) => sameValue(list, stored));
  if (asked && !saving) {
    if (sameValue(asked, stored) || !explained) {
      setAsked(undefined);
      setExpected([]);
    }
  }
  const shown = asked ?? stored;
  const offered = tagVocabulary(listedAgents(added, servedAgents(providers)).map((listed) => allTags[listed] ?? []));
  const path = `agent_tags.${agent}`;

  // Sends `first`, then the edit that waited, one after another, each on the version the store has when it goes.
  const drive = useStableHandler(async (first: string[]) => {
    const report = alertAppDataFailure(t("settings.accounts.tags.saveFailedTitle"));
    // Ends the run: the field shows the stored list again where `shows` says so, else it goes on showing the list last asked for.
    const finish = (shows: boolean) => {
      inFlight.current = false;
      setSaving(false);
      if (shows) {
        setAsked(undefined);
        setExpected([]);
      }
    };
    const explainedNow = () => {
      const settings = store.getState().settings;
      return expectedNow.current.some((list) => sameValue(list, settings.agent_tags[agent] ?? []));
    };
    let list = first;
    while (true) {
      const outcome = await request({
        type: "update_settings",
        agent_tags: { [agent]: list },
        base_versions: baseVersions(store.getState().settings.versions, [path]),
      }).then(() => ({ failed: false as const }), (error: unknown) => ({ failed: true as const, error }));
      const named = outcome.failed ? writeConflict(outcome.error) : undefined;
      if (outcome.failed && named) {
        await whenState(store, (state) => reachedConflict(named, state.settings.versions));
        if (explainedNow()) {
          // The store was behind our own earlier save: the list goes again on the version it has now.
          list = waiting.current ?? list;
          waiting.current = undefined;
          continue;
        }
        // Another client's change, which is shown now; what we asked, and what waited, is dropped.
        waiting.current = undefined;
        finish(true);
        return report(outcome.error);
      }
      if (outcome.failed) report(outcome.error);
      // Whether the edit goes is settled in the same step that finds none waiting, so one made meanwhile cannot be lost.
      const next = waiting.current;
      waiting.current = undefined;
      if (!next) return finish(outcome.failed);
      if (!explainedNow()) {
        // The store holds a list none of our saves explains: another client's, which `next` would overwrite, as it
        // goes on the version the store has now.
        finish(true);
        // The code has catalogued wording in every language, so the message here is never shown.
        return outcome.failed ? undefined : report(new CodedError("changed elsewhere", "write_conflict", { fields: path, versions: "{}" }));
      }
      list = next;
    }
  });
  const change = (tags: string[]) => {
    setAsked(tags);
    const nextExpected = expectedNow.current.length ? [...expectedNow.current, tags] : [stored, tags];
    expectedNow.current = nextExpected;
    setExpected(nextExpected);
    if (inFlight.current) {
      waiting.current = tags;
      return;
    }
    inFlight.current = true;
    setSaving(true);
    void drive(tags);
  };
  const name = isolate(providerLabel(t, providers, agent));
  return (
    <SettingRow
      label={t("settings.accounts.tags.heading")}
      help={<HelpTip label={t("settings.accounts.tags.helpLabel")}>{t("settings.accounts.tags.helpText")}</HelpTip>}
      description={t("settings.accounts.setting.description", { agent: name })}
      below={
        <TagsInput
          hideLabel
          label={t("settings.accounts.tags.label", { agent: name })}
          pickedLabel={t("settings.accounts.tags.picked", { agent: name })}
          value={shown}
          onChange={change}
          suggestions={offered}
          placeholder={t(shown.length ? "settings.accounts.tags.add" : "settings.accounts.tags.none")}
        />
      }
    />
  );
}
