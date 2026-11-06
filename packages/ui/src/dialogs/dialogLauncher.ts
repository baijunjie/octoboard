import { createLauncher } from "../components/hooks/createLauncher";
import type { DialogRequest } from "./dialogRequest";

/** How a control inside another dialog (Settings) opens one of the window's own dialogs, which the window owns so that
 * it outlives the dialog that asked for it. Like `openSettingsFromAnywhere`, a module of its own rather than a context. */
export type OpenDialog = (dialog: DialogRequest) => void;

const launcher = createLauncher<Parameters<OpenDialog>>();

/** Registers the window's way of opening its dialogs, and returns what unregisters it. */
export const registerDialogLauncher: (open: OpenDialog) => () => void = launcher.register;

/** Opens `dialog` in the window; does nothing while no window has registered. */
export const openDialogFromAnywhere: OpenDialog = launcher.open;
