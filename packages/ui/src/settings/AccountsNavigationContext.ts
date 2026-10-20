import { createContext } from "react";

/** Settings navigation requests for the Agent accounts section. `pending` asks it to put the Add agent dialog
 * in front, which the section takes up once (`consume`) as it mounts or, when it is showing already, as the
 * request comes in. The flag is the dialog's, not the section's: the section unmounts when another one is shown,
 * and must not open the dialog again on its return. `dismissForms` counts later navigation requests, which close
 * account forms before showing the destination. A request for Add agent leaves that dialog open. */
export const AccountsNavigationContext = createContext<{ pending: boolean; consume: () => void; dismissForms: number }>({ pending: false, consume: () => {}, dismissForms: 0 });
