import { useSyncExternalStore } from "react";

export const PHONE_QUERY = "(max-width: 767px)";
export const NARROW_QUERY = "(max-width: 1023px)";
export const HOVER_QUERY = "(hover: hover) and (pointer: fine)";

export function matchesMedia(query: string): boolean {
  return window.matchMedia(query).matches;
}

export function useMediaQuery(query: string): boolean {
  return useSyncExternalStore(
    (onChange) => {
      const list = window.matchMedia(query);
      list.addEventListener("change", onChange);
      return () => list.removeEventListener("change", onChange);
    },
    () => matchesMedia(query),
  );
}
