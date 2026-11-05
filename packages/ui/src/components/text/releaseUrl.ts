import { appConfig } from "../../appConfig";

/** The GitHub release of a version; releases are tagged `desktop-v<version>`. */
export function releaseUrl(version: string): string {
  return `${appConfig.repositoryUrl}/releases/tag/desktop-v${version}`;
}
