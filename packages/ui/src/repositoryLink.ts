export type RepositoryBrand = "github" | "gitlab" | "bitbucket";

export interface RepositoryLink {
  url: string;
  host: string;
  brand?: RepositoryBrand;
}

const HOSTS = new Map<string, { host: string; brand: RepositoryBrand }>([
  ["github.com", { host: "GitHub", brand: "github" }],
  ["gitlab.com", { host: "GitLab", brand: "gitlab" }],
  ["bitbucket.org", { host: "Bitbucket", brand: "bitbucket" }],
]);

/** A remote is data from git config, not a browser address. Only web and network git remotes
 * become links; credentials and transport-only ports never reach the browser. */
export function repositoryLink(remote: string | null | undefined): RepositoryLink | undefined {
  if (!remote || /[\s\\]/.test(remote) || [...remote].some((char) => char.charCodeAt(0) < 32 || char.charCodeAt(0) === 127)) return undefined;
  const scp = /^(?:[^/@:]+@)?([a-zA-Z0-9.-]+):(.+)$/.exec(remote);
  let parsed: URL;
  try {
    parsed = new URL(scp && !remote.includes("://") ? `ssh://${scp[1]}/${scp[2].replace(/^\/+/, "")}` : remote);
  } catch {
    return undefined;
  }
  const web = ["https:", "http:"].includes(parsed.protocol);
  if ((!web && !["ssh:", "git:", "git+ssh:", "ssh+git:"].includes(parsed.protocol)) || !parsed.hostname) return undefined;
  // SCP-style local paths such as C:/repo and localhost:repo are not web hosts.
  if (scp && !remote.includes("://") && !parsed.hostname.includes(".")) return undefined;
  const path = parsed.pathname.replace(/\/+$/, "").replace(/\.git$/i, "");
  if (!path || path === "/") return undefined;
  if (!web) {
    try {
      parsed = new URL(`https://${parsed.hostname}${path}`);
    } catch {
      return undefined;
    }
  }
  parsed.username = "";
  parsed.password = "";
  parsed.pathname = path;
  parsed.search = "";
  parsed.hash = "";
  const known = HOSTS.get(parsed.hostname);
  return { url: parsed.href, host: known?.host ?? parsed.hostname, ...(known ? { brand: known.brand } : {}) };
}
