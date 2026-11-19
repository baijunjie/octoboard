import React from "react";

import type { RemoteDaemonsCapability } from "../platform";
import { RemoteDevicesProvider } from "./RemoteDevices";
import { useRemoteDaemons } from "./useRemoteDaemons";

/** What a test of something that reads `useRemoteDevices` (the rail's button) stands it in: the window's part of the
 * page, with the page not shown and the first device selected. */
export function RemoteDevicesTestHost({ capability, children }: { capability: RemoteDaemonsCapability | undefined; children: React.ReactNode }): React.ReactElement {
  const list = useRemoteDaemons(capability);
  if (!capability) return <>{children}</>;
  return (
    <RemoteDevicesProvider capability={capability} list={list} selectedId={list.entries[0]?.id} adding={false} onShow={() => {}} onAttention={() => {}} onSelect={() => {}} onAdd={() => {}} onCancelAdd={() => {}}>
      {children}
    </RemoteDevicesProvider>
  );
}
