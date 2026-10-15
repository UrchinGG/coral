import { useCallback } from "react";
import { useSearchParams } from "react-router-dom";

import { currentRouterState, flushHistoryWrites } from "./history-state";

export type HistoryMode = "replace" | "push";

type UrlStateOptions = { history?: HistoryMode };

export function useUrlParam(name: string, options: UrlStateOptions = {}): readonly [string | null, (value: string | null) => void] {
  const [params, setParams] = useSearchParams();
  const replace = (options.history ?? "replace") === "replace";

  const update = useCallback(
    (value: string | null) => {
      flushHistoryWrites();
      setParams(
        (previous) => {
          const next = new URLSearchParams(previous);
          if (value === null || value === "") next.delete(name);
          else next.set(name, value);
          return next;
        },
        { replace, state: currentRouterState() },
      );
    },
    [name, replace, setParams],
  );

  return [params.get(name), update] as const;
}

export function useUrlEnum<T extends string>(
  name: string,
  values: readonly T[],
  fallback: T,
  options: UrlStateOptions = {},
): readonly [T, (value: T) => void] {
  const [raw, setRaw] = useUrlParam(name, options);
  const value = values.find((candidate) => candidate === raw) ?? fallback;
  const update = useCallback((next: T) => setRaw(next === fallback ? null : next), [fallback, setRaw]);
  return [value, update] as const;
}

export function useUrlFlag(name: string, options: UrlStateOptions = {}): readonly [boolean, (on: boolean) => void] {
  const [raw, setRaw] = useUrlParam(name, options);
  const update = useCallback((on: boolean) => setRaw(on ? "1" : null), [setRaw]);
  return [raw === "1", update] as const;
}
