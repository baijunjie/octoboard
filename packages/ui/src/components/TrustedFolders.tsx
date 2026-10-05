import { Button, Disclosure } from "@heroui/react";
import React from "react";

/** The folders under which Octoboard answers Claude Code's trust prompt for every project, with a
 * way to stop each. Hidden while there are none: there is then nothing to stop. Stopping leaves
 * each project's own consent and every running session as it is. Collapsed until opened. */
export function TrustedFolders({
  directories,
  onRemove,
}: {
  directories: string[];
  onRemove: (path: string) => void;
}): React.ReactElement | null {
  if (directories.length === 0) return null;
  return (
    <Disclosure className="shrink-0 border-t border-separator px-2 py-1">
      <Disclosure.Heading>
        <Disclosure.Trigger
          preventFocusOnPress
          className="flex w-full items-center justify-between rounded px-1 py-1 text-xs text-muted"
        >
          <span title="Octoboard answers Claude Code's trust prompt for every project under these folders.">
            Trusted folders ({directories.length})
          </span>
          <Disclosure.Indicator />
        </Disclosure.Trigger>
      </Disclosure.Heading>
      <Disclosure.Content>
        <Disclosure.Body>
          <ul className="max-h-40 overflow-y-auto">
            {directories.map((path) => (
              <li key={path} className="flex items-center gap-1 text-xs">
                {/* Cut from the start when too long, so the folder's own name stays visible. */}
                <span className="min-w-0 flex-1 truncate text-left" style={{ direction: "rtl" }} title={path}>
                  <bdi>{path}</bdi>
                </span>
                <Button
                  isIconOnly
                  size="sm"
                  variant="ghost"
                  preventFocusOnPress
                  aria-label={`Stop trusting ${path}`}
                  onPress={() => onRemove(path)}
                >
                  ×
                </Button>
              </li>
            ))}
          </ul>
        </Disclosure.Body>
      </Disclosure.Content>
    </Disclosure>
  );
}
