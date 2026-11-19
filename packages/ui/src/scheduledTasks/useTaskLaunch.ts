import { useState } from "react";

import { useLaunchChoice } from "../dialogs/useLaunchChoice";
import type { Console, Project, ScheduledTaskLaunch } from "../protocol";
import { followLaunch, type LaunchFollow } from "./taskDraft";
import { launchOf, launchPresetOf, sameLaunchSettings, type TaskFields } from "./taskFields";

/** The model, effort and switches `launch` holds, as the pickers' overrides. */
function overridesOf(launch: ScheduledTaskLaunch) {
  return { model: launch.model, effort: launch.effort, compatibility: launch.compatibility };
}

/**
 * The launch part of the Scheduled tasks form's draft: the agent, the account and the launch settings, which are the
 * session dialog's launch choice (`useLaunchChoice`), and the `launch` they build with the name and the console session
 * the fields hold (`keepsOwner`: whether the console session to report to is kept, as it is for a project task).
 *
 * The pickers start from the form's base launch (`base`: the saved task's, overlaid with the one a save of the form sent
 * that has not reached it; none for a new task) and are one part of the draft (`TaskDraft`): the launch is the user's
 * edit while what they build differs from the base, and is none once it equals it, whichever picker was used. When the
 * base moves, pickers that built the base before it moved go back to it, so what is saved is never a mix of the saved
 * task's agent and the pickers' model, while pickers that built anything else keep it (`followLaunch`); after a refused
 * save (`keep`) they keep what they hold either way. Within the launch, the agent and the account are the start's until
 * one of the two pickers is used, or after they are picked back to it (a launch that leaves its account to its target is
 * not given the default account by a change to the model), and the model, effort and switches are the start's until one
 * is changed; a start whose agent or account can no longer be picked shows both pickers empty (`gone`), as the session
 * dialog does, and is built as it is until one is picked.
 */
export function useTaskLaunch({ base, console, project, fields, keepsOwner }: {
  base: ScheduledTaskLaunch | undefined;
  console: Console;
  project: Project | undefined;
  fields: Pick<TaskFields, "name" | "boundTo">;
  keepsOwner: boolean;
}) {
  // The base the pickers last started from, and whether the agent or account was picked, or another setting changed,
  // since.
  const [start, setStart] = useState(base);
  const [picked, setPicked] = useState(false);
  const [adjusted, setAdjusted] = useState(false);
  const [follow, setFollow] = useState<LaunchFollow>({ base, keep: false });
  const preset = launchPresetOf(start);
  const choice = useLaunchChoice(console, project, undefined, undefined, preset);
  const pickedBack = preset !== undefined && choice.choice.agent === preset.choice.agent && choice.choice.account === preset.choice.account;
  const startIdentity = start !== undefined && (choice.presetGone || !picked || pickedBack);
  const startSettings = start !== undefined && (choice.presetGone || (!picked && !adjusted));
  const launch: ScheduledTaskLaunch = launchOf(
    fields,
    startIdentity ? start : choice.choice,
    startSettings ? overridesOf(start) : choice.overrides,
    keepsOwner,
  );

  const startFrom = (to: ScheduledTaskLaunch | undefined) => {
    choice.reset(launchPresetOf(to));
    setStart(to);
    setPicked(false);
    setAdjusted(false);
  };
  // Taken during render, so that no render shows the moved base beside pickers that were to follow it.
  const followed = followLaunch(follow, base, launch);
  if (followed) {
    setFollow(followed.follow);
    if (followed.reset) startFrom(base);
  }

  const { account, launch: settings } = choice;
  return {
    launch,
    /** The launch is the user's: for a saved task, what the pickers build differs from the base; for a new one, which
     * has no base, a picker was used. */
    edited: base === undefined ? picked || adjusted : !sameLaunchSettings(launch, base),
    /** Puts the pickers back to the base, for Revert. */
    revert: () => {
      startFrom(base);
      setFollow({ base, keep: false });
    },
    /** The pickers keep what they hold over the next base, which a refused save moves. */
    keep: () => setFollow((current) => ({ ...current, keep: true })),
    /** The agent, the account and the launch settings, as the pickers show and change them. */
    pickers: {
      agent: choice.choice.agent,
      agentOptions: choice.agentOptions,
      /** The start's agent or account can no longer be picked and none has been picked since: both pickers show empty. */
      gone: choice.presetGone,
      pickAgent: (agent: Parameters<typeof choice.setPickedAgent>[0]) => {
        setPicked(true);
        choice.setPickedAgent(agent);
      },
      account: {
        ...account,
        onChange: (key: string) => {
          setPicked(true);
          account.onChange(key);
        },
      },
      settings: {
        ...settings,
        compatibility: {
          ...settings.compatibility,
          onChange: (changes: Parameters<typeof settings.compatibility.onChange>[0]) => {
            setAdjusted(true);
            settings.compatibility.onChange(changes);
          },
        },
        onModelChange: (value: string | null) => {
          setAdjusted(true);
          settings.onModelChange(value);
        },
        onEffortChange: (value: string | null) => {
          setAdjusted(true);
          settings.onEffortChange(value);
        },
      },
    },
  };
}
