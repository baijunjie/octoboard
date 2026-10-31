export const PROJECT_NAME_LIMIT = 100;
export const PROJECT_DESCRIPTION_LIMIT = 2_000;

/** Count the text that will be saved, treating a Unicode code point as one character. */
export function textLength(value: string): number {
  return Array.from(value.trim()).length;
}

export function exceedsTextLimit(value: string, limit: number, initialValue?: string): boolean {
  return value.trim() !== initialValue?.trim() && textLength(value) > limit;
}
