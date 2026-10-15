export function readStored(key: string): unknown {
  try {
    const raw = window.localStorage.getItem(key);
    return raw === null ? null : JSON.parse(raw);
  } catch (error) {
    console.warn(`Could not read ${key} from browser storage`, error);
    return null;
  }
}

export function writeStored(key: string, value: unknown): void {
  try {
    window.localStorage.setItem(key, JSON.stringify(value));
  } catch (error) {
    console.warn(`Could not write ${key} to browser storage`, error);
  }
}
