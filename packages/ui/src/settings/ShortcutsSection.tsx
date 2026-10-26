import React, { useEffect, useId, useRef, useState } from "react";

import { Button } from "../components/Button";
import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../components/StatusAnnouncer";
import { useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { bindingOf, bindingsEqual, describeBinding, isModifierCode, type Binding } from "../shortcuts/binding";
import { ALL_COMMANDS, defaultBinding, isAvailable, SHORTCUT_GROUPS, type CommandId, type ShortcutCommand } from "../shortcuts/commands";
import { recordKeys } from "../shortcuts/recording";
import { BindingKbd } from "../shortcuts/ShortcutKbd";
import { restoreAllBindings, restoreBinding, setBinding, shortcutOverrides, useBindings } from "../shortcuts/store";
import type { ShortcutProblem } from "../shortcuts/validate";
import { IS_MAC } from "../useWindowShortcut";
import { SettingRow } from "./SettingRow";
import { SettingsGroup } from "./SettingsGroup";

/**
 * The keyboard shortcuts: every key the app has, grouped, with the app's own commands rebindable. A
 * row's Change records the next combination pressed (see `recordKeys`): a valid one is saved at
 * once, one that cannot be used says why in the row and keeps recording, and Escape (or Cancel)
 * stops. Everything said is also spoken through the status region. The commands this client does
 * not have (the desktop app's, in a browser) are left out of the list.
 */
export function ShortcutsSection(): React.ReactElement {
  const t = useT();
  const platform = usePlatform();
  const client = { windowChrome: platform.windowChrome !== undefined, nativeMenu: platform.appMenu !== undefined };
  const bindings = useBindings();
  const customised = Object.keys(shortcutOverrides.useValue()).length > 0;
  const [recording, setRecording] = useState<CommandId>();
  const [problem, setProblem] = useState<{ id: CommandId; text: string }>();
  const [announcement, setAnnouncement] = useState<{ text: string; count: number }>();
  const root = useRef<HTMLDivElement>(null);

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
    const modifier = IS_MAC ? "⌘" : "Ctrl";
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
      // ends the recording; it is not a key to bind. ⌃Tab is: it is the console-session switch's key.
      if (event.key === "Tab" && !event.ctrlKey && !event.metaKey) {
        setRecording(undefined);
        setProblem(undefined);
        return "passed";
      }
      // Enter and Space on their own press the focused Cancel; they are not keys to bind either.
      if ((event.key === "Enter" || event.key === " ") && plain && !event.shiftKey) return "passed";
      // A modifier on its own is the start of a combination.
      if (isModifierCode(event.code)) return "consumed";
      const binding = bindingOf(event);
      const found = setBinding(recording, binding);
      if (found) {
        const text = problemText(found);
        setProblem({ id: recording, text });
        speak(text);
      } else {
        setRecording(undefined);
        setProblem(undefined);
        speak(t("settings.shortcuts.announce.saved", { command: name(recording), keys: describeBinding(binding, t) }));
      }
      return "consumed";
    });
  }, [recording, t]);

  const startRecording = (id: CommandId) => {
    setProblem(undefined);
    setRecording(id);
    speak(t("settings.shortcuts.announce.recording", { command: name(id) }));
  };

  // Puts one command's default back, or says what another command holds of it.
  const reset = (id: CommandId): boolean => {
    const found = restoreBinding(id);
    if (found) {
      const text = problemText(found);
      setProblem({ id, text });
      speak(text);
      return false;
    }
    setProblem(undefined);
    speak(t("settings.shortcuts.announce.reset", { command: name(id), keys: describeBinding(defaultBinding(id), t) }), true);
    return true;
  };

  const groups = SHORTCUT_GROUPS.map((group) => ({
    group,
    commands: ALL_COMMANDS.filter((command) => command.group === group && isAvailable(command.availability, client)),
  })).filter(({ commands }) => commands.length > 0);

  return (
    <div ref={root}>
      <StatusAnnouncer text={announcement && (announcement.count % 2 === 0 ? `${announcement.text}\u00A0` : announcement.text)} />
      <div className="flex flex-col items-start gap-3 pb-2 sm:flex-row sm:items-center sm:justify-between sm:gap-6">
        <p className="min-w-0 text-sm text-app-muted">{t("settings.shortcuts.intro")}</p>
        {customised && (
          <Button
            size="sm"
            variant="secondary"
            className="shrink-0"
            onPress={() => {
              // The button goes away with the change, so focus is handed to the list first.
              root.current?.querySelector<HTMLElement>("[data-shortcut-change]")?.focus();
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
        <SettingsGroup key={group} title={t(`settings.shortcuts.group.${group}`)}>
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
  command: ShortcutCommand;
  binding: Binding;
  recording: boolean;
  problem?: string;
  onRecord: (id: CommandId) => void;
  onCancel: () => void;
  /** Puts the command's default back, and returns whether it could. */
  onReset: (id: CommandId) => boolean;
}): React.ReactElement {
  const t = useT();
  const problemId = useId();
  const changeRef = useRef<HTMLButtonElement>(null);
  const label = t(command.label);
  const id = command.id as CommandId;
  const differs = command.rebindable && !bindingsEqual(binding, command.binding);
  return (
    <SettingRow
      // A step below the group heading above it, so the heading reads as what the rows belong to.
      label={<span className="font-normal text-app-muted">{label}</span>}
      description={problem && (
        // Drawn as a field's `FieldError` is, and spoken by the section's status region instead of an alert of its own.
        <span id={problemId} className="block px-1 text-xs wrap-break-word text-danger">{problem}</span>
      )}
    >
      <div className="flex min-h-6 flex-wrap items-center gap-2">
        {recording ? <span className="text-sm text-app-muted">{t("settings.shortcuts.recording")}</span> : <BindingKbd binding={binding} />}
        {command.rebindable ? (
          <>
            <Button
              ref={changeRef}
              data-shortcut-change=""
              size="xs"
              variant="secondary"
              aria-label={t(recording ? "settings.shortcuts.cancelLabel" : "settings.shortcuts.changeLabel", { command: label })}
              aria-describedby={problem ? problemId : undefined}
              onPress={() => (recording ? onCancel() : onRecord(id))}
            >
              {t(recording ? "settings.shortcuts.cancel" : "settings.shortcuts.change")}
            </Button>
            {differs && !recording && (
              <Button
                size="xs"
                variant="ghost"
                aria-label={t("settings.shortcuts.resetLabel", { command: label })}
                aria-describedby={problem ? problemId : undefined}
                onPress={() => {
                  // The button goes away with the change, so focus is handed to this row's Change
                  // first; a reset that is refused leaves the button where it is.
                  if (onReset(id)) changeRef.current?.focus();
                }}
              >
                {t("settings.shortcuts.reset")}
              </Button>
            )}
          </>
        ) : (
          <span className="text-xs text-app-muted">{t("settings.shortcuts.fixed")}</span>
        )}
      </div>
    </SettingRow>
  );
}
