import React, { useEffect, useRef, useState } from "react";

import { AttendedStatusAnnouncer } from "../components/announce/AttendedStatusAnnouncer";
import { DiagnosticBlock } from "../components/alerts/DiagnosticBlock";
import { ErrorAlert } from "../components/alerts/ErrorAlert";
import { settle } from "../components/hooks/settle";
import { onWindowFocusReturn } from "../components/hooks/windowFocus";
import type { Translate } from "../i18n/catalog";
import { InlineReference } from "../i18n/InlineReference";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { CommandLineError, CommandLineState } from "../platform";
import { commandLineError } from "../platform/commandLineError";
import { usePlatform } from "../platform/react";
import { Button } from "../primitives";
import { COMMAND_LINE_TOOL } from "./commandLineTool";
import { SettingRow } from "./SettingRow";
import { useSectionRefocus } from "./useSectionRefocus";

type Change = "install" | "remove";

/** The command-line tool, a link named `octoboardd` to the application's own: where it stands, read when the dialog opens
 * and again whenever the window gets focus back (it can be changed from a terminal), and the actions that fit: install
 * where there is none, reinstall where it runs another copy or leads nowhere (or is fine), remove where there is one.
 * The state, a state that could not be read and a failure of a change are said in the row, never in a dialog; a failure
 * is an alert in the row, worded by the shell's code with its own words behind Details. While a change is out its
 * button is pending and the other disabled, a read of the state made meanwhile is skipped, and the answer of a read
 * that a change overtook is dropped (`asked` counts the reads and changes). The outcome of a change is announced as the
 * state it ends in, through `AttendedStatusAnnouncer` mounted empty with the row: the shape of the Permissions section's
 * states (SA-L in `docs/voiceover-cases.md`) and of a request's answer (SA-P), with focus resting on the pressed
 * button, which no case records; a failure is announced by its alert. Rendered only where the shell offers the tool
 * (`GeneralSection`). */
export function CommandLineSetting(): React.ReactElement | null {
  const t = useT();
  const { commandLine } = usePlatform();
  const [state, setState] = useState<CommandLineState | undefined>();
  const [readFailed, setReadFailed] = useState(false);
  const [changing, setChanging] = useState<Change | undefined>();
  const [failure, setFailure] = useState<{ change: Change; error: CommandLineError } | undefined>();
  // What the last change ended in, said once, and how many there were: the same words twice in a row differ by a
  // no-break space so that the second is said too.
  const [outcome, setOutcome] = useState<{ text: string; count: number } | undefined>();
  const asked = useRef(0);
  const changeOut = useRef(false);
  const installed = state !== undefined && state.state !== "notInstalled";
  // Remove goes away under focus once the tool is not installed.
  useSectionRefocus([installed]);

  useEffect(() => {
    if (!commandLine) return;
    const read = () => {
      if (changeOut.current) return;
      asked.current += 1;
      const moment = asked.current;
      void commandLine.status().then(
        (answer) => {
          if (moment !== asked.current) return;
          setState(answer);
          setReadFailed(false);
        },
        () => {
          if (moment === asked.current) setReadFailed(true);
        },
      );
    };
    read();
    const undoFocus = onWindowFocusReturn(read);
    return () => {
      asked.current += 1;
      undoFocus();
    };
  }, [commandLine]);

  if (!commandLine) return null;

  const change = async (kind: Change) => {
    changeOut.current = true;
    asked.current += 1;
    setChanging(kind);
    setFailure(undefined);
    await settle(
      async () => {
        const answer = await commandLine[kind]();
        setState(answer);
        setReadFailed(false);
        setOutcome((previous) => ({ text: stateText(t, answer), count: (previous?.count ?? 0) + 1 }));
      },
      (cause) => {
        setFailure({ change: kind, error: commandLineError(cause) });
        setOutcome(undefined);
      },
      () => {
        changeOut.current = false;
        asked.current += 1;
        setChanging(undefined);
      },
    );
  };

  const status = state ? (
    <>
      <div>{stateWords(state)}</div>
      {state.state === "installed" && state.pathNote && (
        <div><Message id="settings.commandLine.state.pathNote" params={{ pathVariable: <InlineReference monospace>PATH</InlineReference> }} /></div>
      )}
    </>
  ) : readFailed ? (
    <div>{t("settings.commandLine.state.unreadable")}</div>
  ) : undefined;

  return (
    <SettingRow
      searchId="command-line"
      label={t("settings.commandLine.label")}
      description={<Message id="settings.commandLine.description" params={{ command: <InlineReference monospace>{COMMAND_LINE_TOOL}</InlineReference> }} />}
      status={status}
      below={
        <>
          {failure && (
            <ErrorAlert
              announce
              title={t(failure.change === "install" ? "settings.commandLine.error.installTitle" : "settings.commandLine.error.removeTitle")}
              details={failure.error.detail && failure.error.code !== "foreign_link" ? <DiagnosticBlock>{failure.error.detail}</DiagnosticBlock> : undefined}
            >
              {failureWords(failure.error)}
            </ErrorAlert>
          )}
          <AttendedStatusAnnouncer text={outcome && (outcome.count % 2 === 0 ? `${outcome.text}\u00A0` : outcome.text)} clearAfterRead />
        </>
      }
    >
      <div className="flex items-center gap-2">
        {installed && (
          <Button size="sm" variant="secondary" pending={changing === "remove"} disabled={changing === "install"} onPress={() => void change("remove")}>
            {t("settings.commandLine.remove")}
          </Button>
        )}
        <Button size="sm" variant="secondary" pending={changing === "install"} disabled={changing === "remove" || (state === undefined && !readFailed)} onPress={() => void change("install")}>
          {t(installed ? "settings.commandLine.reinstall" : "settings.commandLine.install")}
        </Button>
      </div>
    </SettingRow>
  );
}

/** Where the tool stands, as a sentence with its path a chip. */
function stateWords(state: CommandLineState): React.ReactElement {
  switch (state.state) {
    case "notInstalled":
      return <Message id="settings.commandLine.state.notInstalled" />;
    case "installed":
      return <Message id="settings.commandLine.state.installed" params={{ path: pathChip(state.path) }} />;
    case "otherCopy":
      return <Message id="settings.commandLine.state.otherCopy" params={{ path: pathChip(state.path) }} />;
    case "unresolved":
      return <Message id="settings.commandLine.state.unresolved" params={{ path: pathChip(state.path) }} />;
  }
}

/** The same as text, for what is announced. */
function stateText(t: Translate, state: CommandLineState): string {
  switch (state.state) {
    case "notInstalled":
      return t("settings.commandLine.state.notInstalled");
    case "installed":
      return t("settings.commandLine.state.installed", { path: isolate(state.path) });
    case "otherCopy":
      return t("settings.commandLine.state.otherCopy", { path: isolate(state.path) });
    case "unresolved":
      return t("settings.commandLine.state.unresolved", { path: isolate(state.path) });
  }
}

/** Why a change failed, by the shell's code. A foreign link's `detail` is the target the link leads to, said in the
 * sentence. A code that speaks of a path but came with none (or a foreign link with no target), and `failed`, say
 * only that the change could not be made: the shell's own words are behind Details. */
function failureWords({ code, path, detail }: CommandLineError): React.ReactElement {
  if (path !== null) {
    if (code === "refused") return <Message id="settings.commandLine.error.refused" params={{ path: pathChip(path) }} />;
    if (code === "not_a_link") return <Message id="settings.commandLine.error.notALink" params={{ path: pathChip(path) }} />;
    if (code === "foreign_link" && detail) return <Message id="settings.commandLine.error.foreignLink" params={{ path: pathChip(path), target: pathChip(detail) }} />;
  }
  return <Message id="settings.commandLine.error.failed" />;
}

function pathChip(path: string): React.ReactElement {
  return <InlineReference monospace wrap>{path}</InlineReference>;
}
