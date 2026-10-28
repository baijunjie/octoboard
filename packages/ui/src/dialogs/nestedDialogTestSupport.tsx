// Shared by the tests of the modals that come up a commit after what raised them (the error alert, a pending
// question), which must stay usable over a dialog opened in turn by another one.
import { useEffect, useState } from "react";

import { Dialog } from "./Dialog";
import { TextInput } from "./TextInput";

/** A "Settings" dialog that opens an "Account settings" dialog over itself from a passive effect as it mounts, as
 * Settings does for View in Settings or an alert's Add agent…: the second dialog mounts a commit after the first. */
export function OpensAnotherDialog(): React.ReactElement {
  const [second, setSecond] = useState(false);
  useEffect(() => setSecond(true), []);
  return (
    <>
      <Dialog title="Settings" onClose={() => {}} footer={null}><p>Sections</p></Dialog>
      {second && <Dialog title="Account settings" onClose={() => {}} footer={null}><TextInput label="Name" value="" onChange={() => {}} autoFocus /></Dialog>}
    </>
  );
}
