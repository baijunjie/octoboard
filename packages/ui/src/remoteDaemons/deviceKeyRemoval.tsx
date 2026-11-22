import React from "react";

import { raiseError } from "../components/alerts/raiseError";
import { ActionableError } from "../errors/actionableError";
import type { Translate } from "../i18n/catalog";
import { InlineReference } from "../i18n/InlineReference";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { RemoteDeviceKeyRemoval } from "../platform";

/**
 * Tells the owner what taking this computer's key back left behind: nothing where the key's line was removed from the
 * server, or none was added, and its private half is gone from this computer. Where the line could not be removed (the
 * server could not be reached, refused the key, or its file does not hold the line), an alert names the line's mark,
 * the file where the shell knows it, and the user and host, so the owner can look for the line there: it may still be
 * there, which the application cannot know, so the alert does not say it is. It says the private key was removed only
 * where it was. Where the system's credential store did not remove the private
 * key, a second alert names the item for the owner to remove there. Where the server's file may have been left
 * incomplete (rewriting it failed, and so did putting its content back), an alert of its own says so and names the
 * copy the server kept, since other keys may be missing from the file until the owner restores it. What the server, the connection or the store said
 * is behind each alert's Error details. Whatever asked for the key to be taken back (removing the remote daemon,
 * pointing it elsewhere, turning the key off) has been done all the same.
 */
export function reportKeyRemoval(t: Translate, removal: RemoteDeviceKeyRemoval): void {
  const { line, privateKeyLeft } = removal;
  if (line.result === "left_on_host") {
    const { mark, file, host, user, detail } = line;
    const text = { host: isolate(host), user: isolate(user), mark: isolate(mark) };
    const message = file === null ? t("remoteDaemons.deviceKey.left.body.unknownFile", text) : t("remoteDaemons.deviceKey.left.body", { ...text, file: isolate(file) });
    // The sentence about the private key belongs to the alert only where the key did go.
    const removed = privateKeyLeft === null ? `${t("common.sentenceSeparator")}${t("remoteDaemons.deviceKey.left.privateKeyRemoved")}` : "";
    raiseError(new ActionableError(`${message}${removed}`, "none", { detail }), {
      title: t("remoteDaemons.deviceKey.left.title"),
      keepTitle: true,
      content: <DeviceKeyLineLeft line={line} privateKeyRemoved={privateKeyLeft === null} />,
    });
  }
  if (line.result === "file_incomplete") {
    // The one outcome that can cost the owner their other keys: it says where the whole copy is.
    const { host, user, copy, detail } = line;
    raiseError(new ActionableError(t("remoteDaemons.deviceKey.incomplete.body", { host: isolate(host), user: isolate(user), copy: isolate(copy) }), "none", { detail }), {
      title: t("remoteDaemons.deviceKey.incomplete.title"),
      keepTitle: true,
      content: <DeviceKeyFileIncomplete line={line} />,
    });
  }
  if (privateKeyLeft !== null) {
    const { service, account, host, user, detail } = privateKeyLeft;
    raiseError(
      new ActionableError(t("remoteDaemons.deviceKey.privateKeyLeft.body", { service: isolate(service), account: isolate(account), host: isolate(host), user: isolate(user) }), "none", { detail }),
      {
        title: t("remoteDaemons.deviceKey.privateKeyLeft.title"),
        keepTitle: true,
        content: <DevicePrivateKeyLeft left={privateKeyLeft} />,
      },
    );
  }
}

function literal(text: string): React.ReactElement {
  return <InlineReference monospace dir="ltr" wrap>{text}</InlineReference>;
}

/** What is said of a key's line that could not be taken off its server: its mark, the file where the shell knows it,
 * and the user and host, so the owner can look for the line there, and, where `privateKeyRemoved`, that the private key
 * did go. The one wording of it, for the alert above and for the clean-up before an uninstall's result
 * (`settings/UninstallCleanup.tsx`). */
export function DeviceKeyLineLeft({ line, privateKeyRemoved }: { line: Extract<RemoteDeviceKeyRemoval["line"], { result: "left_on_host" }>; privateKeyRemoved: boolean }): React.ReactElement {
  const t = useT();
  const chips = {
    host: literal(line.host),
    user: literal(line.user),
    // Literal text the owner looks for in the file, in whatever script the computer's name is written.
    mark: <InlineReference monospace dir="auto" wrap>{line.mark}</InlineReference>,
  };
  return (
    <>
      {line.file === null ? <Message id="remoteDaemons.deviceKey.left.body.unknownFile" params={chips} /> : <Message id="remoteDaemons.deviceKey.left.body" params={{ ...chips, file: literal(line.file) }} />}
      {privateKeyRemoved && `${t("common.sentenceSeparator")}${t("remoteDaemons.deviceKey.left.privateKeyRemoved")}`}
    </>
  );
}

/** What is said of a server's authorized keys left possibly incomplete by a removal cut short: that keys may be
 * missing from them, and where the server keeps their earlier content to restore them from. The one wording of it, as
 * `DeviceKeyLineLeft` is of the line. */
export function DeviceKeyFileIncomplete({ line }: { line: Extract<RemoteDeviceKeyRemoval["line"], { result: "file_incomplete" }> }): React.ReactElement {
  return <Message id="remoteDaemons.deviceKey.incomplete.body" params={{ host: literal(line.host), user: literal(line.user), copy: literal(line.copy) }} />;
}

/** What is said of a private key the system's credential store did not remove: the item to remove there. The one
 * wording of it, as `DeviceKeyLineLeft` is of the line. */
export function DevicePrivateKeyLeft({ left }: { left: NonNullable<RemoteDeviceKeyRemoval["privateKeyLeft"]> }): React.ReactElement {
  return <Message id="remoteDaemons.deviceKey.privateKeyLeft.body" params={{ service: literal(left.service), account: literal(left.account), host: literal(left.host), user: literal(left.user) }} />;
}
