import type { ManifestEntry, ResolvedEntry } from "./entries.mjs";

export * from "./entries.mjs";
export function viewportProblem(entry: ResolvedEntry, scenario: { id: string; width?: number; minWidth?: number; height?: number }): string | undefined;
export function selectEntries<T extends { id: string }>(entries: T[], patterns: string[]): T[];
export function frameUrl(origin: string, entry: ResolvedEntry): string;
export function loadManifest(dir?: string): Promise<ManifestEntry[]>;
