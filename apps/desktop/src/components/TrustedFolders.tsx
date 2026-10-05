import React from "react";

/** The folders under which Octoboard answers Claude Code's trust prompt for every project, with a
 * way to stop each. Hidden while there are none: there is then nothing to stop. Stopping leaves
 * each project's own consent and every running session as it is. */
export function TrustedFolders({
  directories,
  onRemove,
}: {
  directories: string[];
  onRemove: (path: string) => void;
}): React.ReactElement | null {
  if (directories.length === 0) return null;
  return (
    <details className="sidebar-trusted">
      <summary title="Octoboard answers Claude Code's trust prompt for every project under these folders.">
        Trusted folders ({directories.length})
      </summary>
      <ul>
        {directories.map((path) => (
          <li key={path}>
            {/* Cut from the start when too long, so the folder's own name stays visible. */}
            <span className="sidebar-trusted-path" title={path}>
              <bdi>{path}</bdi>
            </span>
            <button
              type="button"
              className="sidebar-trusted-remove"
              aria-label={`Stop trusting ${path}`}
              title="Stop trusting this folder. Projects' own consents and running sessions are kept."
              onClick={() => onRemove(path)}
            >
              ×
            </button>
          </li>
        ))}
      </ul>
    </details>
  );
}
