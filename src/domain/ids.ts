let fallbackCounter = 0;

/** Globally unique, prefixed identifier (e.g. `q_3f0c…`). Prefixes make logs readable. */
export function createId(prefix: string): string {
  const unique =
    globalThis.crypto?.randomUUID?.() ??
    `${Date.now().toString(36)}-${(fallbackCounter++).toString(36)}-${Math.random()
      .toString(36)
      .slice(2, 10)}`;
  return `${prefix}_${unique}`;
}
