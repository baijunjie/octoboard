import React, { useRef, useState } from "react";

import { AGENT_LABEL, listedAgents } from "../../agents/agents";
import { alertAppDataFailure } from "../../components/alerts/raiseError";
import { TagsInput } from "../../components/form/TagsInput";
import { tagVocabulary } from "../../components/form/tagVocabulary";
import { useT } from "../../i18n/react";
import type { Agent } from "../../protocol";
import { sameValue, useDaemon, useDaemonStore } from "../../store";
import { SettingRow } from "../SettingRow";

/** An agent's Tags row, at the top of its tab: free-form labels saying what the agent is good for,
 * which apply to every account of the agent (see "Agent tags" in
 * `docs/product/per-agent-settings.md`). The tags offered are those the added agents carry (every
 * agent while the list is undecided); there is no registry of them. Saved on every change as the
 * whole list, which the daemon normalises. The field shows the list last asked for while any save
 * is outstanding, so tags added in quick succession build on one another and no broadcast of an
 * earlier save shows in between. Once none is outstanding it goes on showing it, since the reply
 * may come before the broadcast of the change, until the store's list equals it or is a value none
 * of our saves explains (the stored list the first save started from, or a list we sent, is
 * explained; anything else is a concurrent writer's or a different normalisation), which is then
 * shown and built on by the next edit. A foreign write that equals a value we sent, or the list
 * the first save started from, is not told apart from ours. When the last outstanding save fails it
 * is reported and the stored tags show again; an earlier failure with a later save outstanding
 * keeps the list asked for, which that later save carries whole. */
export function AgentTagsSetting({ agent }: { agent: Agent }): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const added = useDaemonStore((s) => s.settings.added_agents);
  const allTags = useDaemonStore((s) => s.settings.agent_tags);
  const [asked, setAsked] = useState<string[]>();
  // The lists our outstanding and settled saves explain: the stored list the first one started
  // from, then each one sent. Empty exactly when `asked` is unset.
  const [expected, setExpected] = useState<string[][]>([]);
  const [outstanding, setOutstanding] = useState(0);
  const pending = useRef(0);
  const stored = allTags[agent] ?? [];
  if (asked && outstanding === 0) {
    if (sameValue(asked, stored) || !expected.some((list) => sameValue(list, stored))) {
      setAsked(undefined);
      setExpected([]);
    }
  }
  const shown = asked ?? stored;
  const offered = tagVocabulary(listedAgents(added).map((listed) => allTags[listed] ?? []));
  const change = (tags: string[]) => {
    setAsked(tags);
    setExpected((prior) => (prior.length ? [...prior, tags] : [stored, tags]));
    pending.current += 1;
    setOutstanding(pending.current);
    void request({ type: "update_settings", agent_tags: { [agent]: tags } }).then(
      () => {
        pending.current -= 1;
        setOutstanding(pending.current);
      },
      (error: unknown) => {
        pending.current -= 1;
        setOutstanding(pending.current);
        if (pending.current === 0) {
          setAsked(undefined);
          setExpected([]);
        }
        alertAppDataFailure(t("settings.accounts.tags.saveFailedTitle"))(error);
      },
    );
  };
  const name = AGENT_LABEL[agent];
  return (
    <SettingRow
      label={t("settings.accounts.tags.heading")}
      description={t("settings.accounts.approval.description", { agent: name })}
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
