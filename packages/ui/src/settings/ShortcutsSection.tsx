import { LockKeyhole, RotateCcw, X } from "lucide-react";
import React, { useContext, useEffect, useId, useRef, useState } from "react";

import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../components/announce/StatusAnnouncer";
import { IndicatorTooltip } from "../components/hint/IndicatorTooltip";
import { TitledControl } from "../components/hint/TitledControl";
import { useShortcutScopeDescription } from "../components/hooks/useShortcutScopeDescription";
import { useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { bindingOf, describeBinding, isModifierCode, sameKeys, type Binding } from "../shortcuts/binding";
import { ALL_COMMANDS, clientCapabilities, defaultBinding, isAvailable, SHORTCUT_GROUPS, type CommandId, type ShortcutCommand, type ShortcutCommandId } from "../shortcuts/commands";
import { recordKeys } from "../shortcuts/recording";
import { BindingKbd } from "../shortcuts/ShortcutKbd";
import { restoreAllBindings, restoreBinding, setBinding, shortcutOverrides, useBindings } from "../shortcuts/store";
import type { ShortcutProblem } from "../shortcuts/validate";
import { SettingRow } from "./SettingRow";
import { SettingsGroup } from "./SettingsGroup";
import { SettingsFocusContext } from "./useSectionRefocus";
import { Button } from "../primitives";

/**
 * The keyboard shortcuts: every key the app has, grouped, with the app's own commands rebindable. A
 * row's shortcut button records the next combination pressed (see `recordKeys`): a valid one is saved at
 * once, one that cannot be used says why in the row and keeps recording, and Escape (or pressing the
 * shortcut button again) stops. Everything said is also spoken through the status region. The commands this client does
 * not have (the desktop app's, in a browser) are left out of the list.
 */
export function ShortcutsSection(): React.ReactElement {
  const t = useT();
  const platform = usePlatform();
  const shortcutScope = useShortcutScopeDescription();
  const { host } = platform;
  const client = clientCapabilities(platform);
  const bindings = useBindings();
  const customised = Object.keys(shortcutOverrides.useValue()).length > 0;
  const [recording, setRecording] = useState<CommandId>();
  const [problem, setProblem] = useState<{ id: CommandId; text: string }>();
  const [announcement, setAnnouncement] = useState<{ text: string; count: number }>();
  const root = useRef<HTMLDivElement>(null);
  const sectionTab = useContext(SettingsFocusContext);

  // The status region speaks a change of its text, so the same words said again (a refusal that
  // repeats) differ by a trailing no-break space on every other turn.
  // `afterFocusMove` holds the text back until the move of focus that came with it has been spoken,
  // which would otherwise replace it; a newer message cancels one still waiting.
  const pending = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(pending.current), []);
  const speak = (text: string, afterFocusMove = false) => {
    clearTimeout(pending.current);
    const write = () => setAnnouncement((previous) => ({ text, count: (previous?.count ?? 0) + 1 }));
    if (afterFocusMove) pending.current = setTimeout(write, AFTER_FOCUS_MOVE_DELAY_MS);
    else write();
  };
  const name = (id: string) => t(ALL_COMMANDS.find((command) => command.id === id)!.label);
  const problemText = (found: ShortcutProblem) => {
    const modifier = host.modifier === "meta" ? "⌘" : "Ctrl";
    switch (found.reason) {
      case "conflict": {
        const holder = ALL_COMMANDS.find((command) => command.id === found.with)!;
        return t(isAvailable(holder.availability, client) ? "settings.shortcuts.problem.conflict" : "settings.shortcuts.problem.conflictDesktop", { command: name(found.with) });
      }
      case "needsModifier": return t("settings.shortcuts.problem.needsModifier", { modifier });
      case "terminalControl": return t("settings.shortcuts.problem.terminalControl", { modifier });
      case "unsupportedKey": return t("settings.shortcuts.problem.unsupportedKey");
      case "shiftedKey": return t("settings.shortcuts.problem.shiftedKey");
      case "editingKey": return t("settings.shortcuts.problem.editingKey", { modifier });
      case "reservedKey": return t("settings.shortcuts.problem.reservedKey");
      case "osReserved": return t("settings.shortcuts.problem.osReserved");
      case "interrupt": return t("settings.shortcuts.problem.interrupt");
    }
  };

  useEffect(() => {
    if (!recording) return;
    return recordKeys((event) => {
      if (event.repeat) return "consumed";
      const plain = !event.ctrlKey && !event.metaKey && !event.altKey;
      if (event.key === "Escape" && plain && !event.shiftKey) {
        setRecording(undefined);
        setProblem(undefined);
        speak(t("settings.shortcuts.announce.cancelled"));
        return "consumed";
      }
      // Tab moves on as it always does (⌥Tab too, which on macOS is how a button is reached), which
      // ends the recording; it is not a key to bind. Control with Tab is: it is the console-session switch's key.
      if (event.key === "Tab" && !event.ctrlKey && !event.metaKey) {
        setRecording(undefined);
        setProblem(undefined);
        return "passed";
      }
      // Enter and Space on their own press the focused shortcut button, which cancels; they are not keys to bind either.
      if ((event.key === "Enter" || event.key === " ") && plain && !event.shiftKey) return "passed";
      // A modifier on its own is the start of a combination.
      if (isModifierCode(event.code)) return "consumed";
      const binding = bindingOf(event, host);
      const found = setBinding(recording, binding, host);
      if (found) {
        const text = problemText(found);
        setProblem({ id: recording, text });
        speak(text);
      } else {
        setRecording(undefined);
        setProblem(undefined);
        speak(t("settings.shortcuts.announce.saved", { command: name(recording), keys: describeBinding(binding, t, host) }));
      }
      return "consumed";
    });
  }, [recording, t, host]);

  const startRecording = (id: CommandId) => {
    setProblem(undefined);
    setRecording(id);
    speak(t("settings.shortcuts.announce.recording", { command: name(id) }));
  };

  // Puts one command's default back, or says what another command holds of it.
  const reset = (id: CommandId): boolean => {
    const found = restoreBinding(id, host);
    if (found) {
      const text = problemText(found);
      setProblem({ id, text });
      speak(text);
      return false;
    }
    setProblem(undefined);
    speak(t("settings.shortcuts.announce.reset", { command: name(id), keys: describeBinding(defaultBinding(id), t, host) }), true);
    return true;
  };

  const groups = SHORTCUT_GROUPS.map((group) => ({
    group,
    commands: ALL_COMMANDS.filter((command) => command.group === group && isAvailable(command.availability, client)),
  })).filter(({ commands }) => commands.length > 0);

  return (
    <div ref={root}>
      <StatusAnnouncer text={announcement && (announcement.count % 2 === 0 ? `${announcement.text}\u00A0` : announcement.text)} />
      <div className="flex flex-col items-start gap-3 pb-2 tablet:flex-row tablet:items-center tablet:justify-between tablet:gap-6">
        <p className="min-w-0 text-sm text-app-muted">{`${t("settings.shortcuts.description")}${t("common.sentenceSeparator")}${shortcutScope}`}</p>
        {customised && (
          <Button
            size="sm"
            variant="secondary"
            className="shrink-0 max-tablet:self-end-safe"
            onPress={() => {
              // The button goes away with the change, so focus is handed to the list first: to the first row a search in
              // Settings leaves shown, or to the section's tab while it hides them all.
              const shown = [...root.current?.querySelectorAll<HTMLElement>("[data-shortcut-change]") ?? []].find((change) => !change.closest("[hidden], [data-list-motion-ghost]"));
              (shown ?? sectionTab())?.focus();
              setRecording(undefined);
              setProblem(undefined);
              restoreAllBindings();
              speak(t("settings.shortcuts.announce.restoredAll"), true);
            }}
          >
            {t("settings.shortcuts.restoreAll")}
          </Button>
        )}
      </div>
      {groups.map(({ group, commands }) => (
        <SettingsGroup key={group} title={`settings.shortcuts.group.${group}`}>
          {commands.map((command) => (
            <ShortcutRow
              key={command.id}
              command={command}
              binding={command.rebindable ? bindings[command.id as CommandId] : command.binding}
              recording={recording === command.id}
              problem={problem?.id === command.id ? problem.text : undefined}
              onRecord={startRecording}
              onCancel={() => {
                setRecording(undefined);
                setProblem(undefined);
                speak(t("settings.shortcuts.announce.cancelled"));
              }}
              onReset={reset}
            />
          ))}
        </SettingsGroup>
      ))}
    </div>
  );
}

function ShortcutRow({
  command,
  binding,
  recording,
  problem,
  onRecord,
  onCancel,
  onReset,
}: {
  command: ShortcutCommand & { id: ShortcutCommandId };
  binding: Binding;
  recording: boolean;
  problem?: string;
  onRecord: (id: CommandId) => void;
  onCancel: () => void;
  /** Puts the command's default back, and returns whether it could. */
  onReset: (id: CommandId) => boolean;
}): React.ReactElement {
  const t = useT();
  const { host } = usePlatform();
  const problemId = useId();
  const bindingDescriptionId = useId();
  const promptId = useId();
  const cancelId = useId();
  const changeRef = useRef<HTMLButtonElement>(null);
  const label = t(command.label);
  const id = command.id as CommandId;
  const differs = command.rebindable && !sameKeys(binding, command.binding, host);
  const actionLabel = t(recording ? "settings.shortcuts.cancelLabel" : "settings.shortcuts.changeLabel", { command: label });
  return (
    <SettingRow
      searchId={`shortcut.${command.id}`}
      // A step below the group heading above it, so the heading reads as what the rows belong to.
      label={<span className="font-normal text-app-muted">{label}</span>}
      description={problem && (
        // Drawn as a field's `FieldError` is, and spoken by the section's status region instead of an alert of its own.
        <span id={problemId} className="block px-1 text-xs wrap-break-word text-danger">{problem}</span>
      )}
    >
      <div className="grid grid-cols-[minmax(0,1fr)_1.5rem] items-center gap-2">
        {command.rebindable ? (
          <>
            <span id={bindingDescriptionId} hidden>{describeBinding(binding, t, host)}</span>
            <TitledControl title={actionLabel}>
              <Button
                ref={changeRef}
                data-shortcut-change=""
                size="sm"
                variant="ghost"
                // An approved departure from HeroUI (`docs/ui-standard.md`, "Do not change how a HeroUI component looks"): the ghost button
                // draws no fill of its own, so only the key caps show; hover and press darken the caps themselves, or the recording
                // prompt's text.
                className="h-auto min-h-6 min-w-0 justify-self-end rounded-md p-0 whitespace-normal [--button-bg-hover:transparent] [--button-bg-pressed:transparent]"
                // While recording, the visible prompt leads the accessible name, followed by what pressing does.
                label={recording ? undefined : actionLabel}
                labelledBy={recording ? `${promptId} ${cancelId}` : undefined}
                // Never left undefined: an empty value keeps the tooltip, which repeats the name, from being read as
                // the description too.
                describedBy={[!recording && bindingDescriptionId, problem && problemId].filter(Boolean).join(" ")}
                onPress={() => (recording ? onCancel() : onRecord(id))}
              >
                {recording ? (
                  <span className="flex items-center gap-2 px-1.5 text-end text-sm text-app-muted transition-colors motion-reduce:transition-none in-ui-hovered:text-foreground in-ui-pressed:text-foreground">
                    <span id={promptId}>{t("settings.shortcuts.recording")}</span>
                    <span id={cancelId} className="sr-only">{actionLabel}</span>
                    <X aria-hidden="true" className="size-3.5 shrink-0" />
                  </span>
                ) : <BindingKbd binding={binding} className="transition-colors in-ui-hovered:bg-(--default-hover) in-ui-hovered:text-foreground in-ui-pressed:bg-(--default-hover) in-ui-pressed:text-foreground" />}
              </Button>
            </TitledControl>
          </>
        ) : (
          <span className="justify-self-end"><BindingKbd binding={binding} /></span>
        )}
        <div className="flex size-6 items-center justify-center">
          {/* A short tooltip, as the row's name sits beside it; the accessible name keeps the full wording. */}
          {differs && !recording && (
            <TitledControl title={t("settings.shortcuts.reset")}>
              <Button
                size="xs"
                iconOnly
                variant="ghost"
                label={t("settings.shortcuts.resetLabel", { command: label })}
                describedBy={problem ? problemId : ""}
                onPress={() => {
                  // Reset removes its own button; keep focus on this row's shortcut instead.
                  if (onReset(id)) changeRef.current?.focus();
                }}
              >
                <RotateCcw aria-hidden="true" className="size-3.5" />
              </Button>
            </TitledControl>
          )}
          {/* "Fixed" is the lock's tooltip and name only, so a sighted keyboard user does not see it; kept on purpose,
            as the introduction already says these keys cannot be changed. */}
          {!command.rebindable && (
            <IndicatorTooltip tooltip={t("settings.shortcuts.fixed")} label={t("settings.shortcuts.fixed")} className="inline-flex text-app-muted">
              <LockKeyhole aria-hidden="true" className="size-3.5" />
            </IndicatorTooltip>
          )}
        </div>
      </div>
    </SettingRow>
  );
}
