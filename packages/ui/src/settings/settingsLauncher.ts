import { createLauncher } from "../components/hooks/createLauncher";
import type { SettingsRequestOptions, SettingsSectionId } from "./sections";

/** How a control anywhere in the page opens the Settings dialog, which the window owns: on `section`, with the
 * extras `options` asks for. A module of its own rather than a context because an error
 * alert is rendered beside the window, not inside it. */
export type OpenSettings = (section: SettingsSectionId, options?: SettingsRequestOptions) => void;

const launcher = createLauncher<Parameters<OpenSettings>>();

/** Registers the window's way of opening Settings, and returns what unregisters it. */
export const registerSettingsLauncher: (open: OpenSettings) => () => void = launcher.register;

/** Opens Settings; does nothing while no window has registered. */
export const openSettingsFromAnywhere: OpenSettings = launcher.open;
