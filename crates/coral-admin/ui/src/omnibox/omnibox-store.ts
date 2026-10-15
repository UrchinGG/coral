import { useSyncExternalStore } from "react";

type OmniboxState = { paletteOpen: boolean; focusRequest: number };

const listeners = new Set<() => void>();
let state: OmniboxState = { paletteOpen: false, focusRequest: 0 };

export function useOmniboxState(): OmniboxState {
  return useSyncExternalStore(subscribe, () => state);
}

export function openPalette(): void {
  update({ paletteOpen: true });
}

export function closePalette(): void {
  update({ paletteOpen: false });
}

export function togglePalette(): void {
  update({ paletteOpen: !state.paletteOpen });
}

export function focusOmnibox(): void {
  update({ focusRequest: state.focusRequest + 1 });
}

function update(patch: Partial<OmniboxState>): void {
  state = { ...state, ...patch };
  listeners.forEach((listener) => listener());
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}
