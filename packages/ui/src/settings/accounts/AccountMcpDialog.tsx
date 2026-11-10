import React, { useRef } from "react";

import { Dialog } from "../../components/dialog/Dialog";
import { MarkLineBox } from "../../components/status/MarkLineBox";
import { useT } from "../../i18n/react";
import type { Agent } from "../../protocol";
import { useAccountQuota } from "../../quota/useAccountQuota";
import { AccountHeading, useAccountDialogSubject } from "./accountDialogSubject";
import { AccountLoginAnnouncer, useSucceededAccountLogin } from "./AccountLogin";
import { McpInventory, McpInventoryAside } from "./AccountMcpInventory";
import { useAccountMcpInventory } from "./useAccountMcpInventory";

/**
 * One account's platform connectors and local MCP servers in a modal of its own, opened from its row's actions menu
 * (`account` null for the agent's default account), for a provider that can list them (`useReadsMcpInventory`). The
 * title is plain ("Connectors and MCP"); the body's first line names the account, with the inventory's
 * Refresh button and when it was read at its end, so it is clear which account the lists are for. Then the inventory
 * itself (`McpInventory`): read as the dialog opens and again from Refresh, the last reading staying while the next
 * runs, with a link to the platform's connector settings and, for a server the agent signs in to itself, its Sign in
 * or Sign out. It has no footer: the header's close button, Escape and a click outside close it. It closes if the
 * account is removed, and a reading that lands after it has gone is dropped.
 */
export function AccountMcpDialog({ agent, account, onClose }: {
  agent: Agent;
  account: string | null;
  onClose: () => void;
}): React.ReactElement | null {
  const t = useT();
  const { name, gone } = useAccountDialogSubject(agent, account, onClose);
  const succeeded = useSucceededAccountLogin(agent, account);
  const quota = useAccountQuota(agent, account);
  const mcp = useAccountMcpInventory(agent, account, !gone);
  const refresh = useRef<HTMLButtonElement>(null);
  if (gone) return null;
  return (
    <Dialog
      size="wide"
      title={t("settings.accounts.mcp.title")}
      onClose={onClose}
      footer={null}
    >
      <div className="flex flex-col gap-4">
        <div className="flex items-start gap-1">
          <div className="min-w-0"><AccountHeading name={name} quota={quota} /></div>
          {/* One name line tall, so the aside stays centred on the first line when the email adds a second. */}
          <div className="ms-auto shrink-0">
            <MarkLineBox size="text-sm"><McpInventoryAside state={mcp} refreshButton={refresh} /></MarkLineBox>
          </div>
        </div>
        <AccountLoginAnnouncer login={succeeded} name={name} />
        <McpInventory agent={agent} state={mcp} refreshButton={refresh} />
      </div>
    </Dialog>
  );
}
