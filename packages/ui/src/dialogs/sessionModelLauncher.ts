import { createLauncher } from "../components/hooks/createLauncher";

/** How a control outside the window (an error alert, rendered beside it) opens Change model for a session, which the
 * window owns: the session is looked up by its ID, and nothing happens for one that is gone or while another dialog is
 * open. Like `openSettingsFromAnywhere`, a module of its own rather than a context. */
export type OpenSessionModel = (sessionId: string) => void;

const launcher = createLauncher<Parameters<OpenSessionModel>>();

/** Registers the window's way of opening Change model, and returns what unregisters it. */
export const registerSessionModelLauncher: (open: OpenSessionModel) => () => void = launcher.register;

/** Opens Change model for `sessionId`; does nothing while no window has registered. */
export const openSessionModelFromAnywhere: OpenSessionModel = launcher.open;
