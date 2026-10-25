/** One package the application ships, as `scripts/generate-licenses.mjs` writes it. */
export interface LicensedPackage {
  name: string;
  version: string;
  ecosystem: "npm" | "cargo";
  /** The SPDX expression the package declares, or "See license file". */
  license: string;
  /** Positions in `ThirdPartyLicenses.texts` of the license files the package ships; none when it ships no text. */
  texts: number[];
}

export interface ThirdPartyLicenses {
  /** The distinct license texts; a text several packages share is held once. */
  texts: string[];
  packages: LicensedPackage[];
}

/** The list the build generated (`licenses.generated.json`, which is not committed), or the committed empty one
 * (`licenses.fallback.json`) before the generator has run. Both are imported dynamically, so the list, a megabyte
 * of text, is a chunk of its own that nothing loads until the dialog that shows it opens; the glob names the
 * generated file without requiring it to exist. */
export async function loadThirdPartyLicenses(): Promise<ThirdPartyLicenses> {
  const generated = import.meta.glob<ThirdPartyLicenses>("./licenses.generated.json", { import: "default" });
  const load = generated["./licenses.generated.json"];
  if (load) return load();
  return (await import("./licenses.fallback.json")).default as ThirdPartyLicenses;
}

/** The packages whose name or license contains `query`, ignoring case and the space around it. */
export function filterPackages(packages: readonly LicensedPackage[], query: string): LicensedPackage[] {
  const needle = query.trim().toLowerCase();
  if (needle === "") return [...packages];
  return packages.filter((entry) => entry.name.toLowerCase().includes(needle) || entry.license.toLowerCase().includes(needle));
}
