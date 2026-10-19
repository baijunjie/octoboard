import { createContext } from "react";

/** What Settings has asked of the Agent accounts section's Add agent dialog. `pending` is a request to put it in
 * front, which the section takes up once (`consume`) as it mounts or, when it is showing already, as the request
 * comes in. The flag is the dialog's, not the section's: the section unmounts when another one is shown, and must
 * not open the dialog again on its return. `dismiss` counts the requests to close it, which a later request for
 * anything else makes so that what it shows is not left behind a modal. */
export const AddAgentOnOpenContext = createContext<{ pending: boolean; consume: () => void; dismiss: number }>({ pending: false, consume: () => {}, dismiss: 0 });
