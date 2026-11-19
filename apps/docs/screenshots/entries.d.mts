export interface ScreenshotEntry {
  id: string;
  scenario: string;
  width?: number;
  height?: number;
  scale?: 1 | 2;
  clip?: string | { selector: string; padding?: number };
  keep?: string | string[];
  hover?: string;
}
export interface ResolvedEntry {
  id: string;
  scenario: string;
  width: number;
  height: number;
  scale: 1 | 2;
  clip: { selector: string; padding: number } | undefined;
  keep: string[] | undefined;
  hover: string | undefined;
}
export type ManifestEntry = ResolvedEntry & { group: string };
export const GROUPS: string[];
export const DEFAULT_WIDTH: number;
export const DEFAULT_HEIGHT: number;
export const DEFAULT_SCALE: 2;
export const DEFAULT_CLIP_PADDING: 24;
export function normalizeEntry(entry: unknown, where: string): ResolvedEntry;
export function normalizeManifest(groups: Record<string, unknown>): ManifestEntry[];
