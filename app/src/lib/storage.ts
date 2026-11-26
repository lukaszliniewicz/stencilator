/**
 * The browser's local storage, which may be missing, full or switched off. Nothing the
 * app keeps there is essential, so failures fall back to the defaults silently.
 */
export function readStored(key: string): unknown {
  try {
    const text = localStorage.getItem(key);
    return text === null ? undefined : (JSON.parse(text) as unknown);
  } catch {
    return undefined;
  }
}

export function writeStored(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Private windows and full storage: the settings just are not remembered.
  }
}
