import { useNavigate } from "react-router-dom";

import { toast } from "../kit/toast";
import { copyText } from "../lib/clipboard";
import { historyStateWith, useHistoryValue } from "../lib/history-state";
import { useHotkey } from "../lib/hotkeys";

export type ListHotkeys<T> = {
  rows: readonly T[];
  rowKey: (row: T) => string;
  selectedKey: string | null;
  onSelect: (key: string | null) => void;
  onOpen: (row: T) => void;
  enabled?: boolean;
};

type ListContext = { paths: readonly string[]; index: number };

const LIST_CONTEXT_KEY = "list";

export function useListHotkeys<T>({ rows, rowKey, selectedKey, onSelect, onOpen, enabled = true }: ListHotkeys<T>): void {
  const position = selectedKey === null ? -1 : rows.findIndex((row) => rowKey(row) === selectedKey);
  const step = (delta: number) => {
    if (rows.length === 0) return;
    const next = position < 0 ? 0 : Math.min(rows.length - 1, Math.max(0, position + delta));
    onSelect(rowKey(rows[next]));
  };
  useHotkey("j", () => step(1), { enabled });
  useHotkey("k", () => step(-1), { enabled });
  useHotkey("Enter", () => step(0), { enabled });
  useHotkey("o", () => position >= 0 && onOpen(rows[position]), { enabled });
  useHotkey("Escape", () => onSelect(null), { enabled: enabled && selectedKey !== null });
}

export function useCopyHotkey(text: string | null, label: string): void {
  useHotkey(
    "c",
    () => {
      if (text === null) return;
      void copyText(text).then((outcome) =>
        outcome === "copied" ? toast.ok(`Copied ${label}.`) : toast.error(`Clipboard access is blocked; select the ${label} and copy it by hand.`),
      );
    },
    { enabled: text !== null },
  );
}

export function listContextState(paths: readonly string[], index: number): Record<string, unknown> {
  return historyStateWith({ [LIST_CONTEXT_KEY]: JSON.stringify({ paths, index } satisfies ListContext) });
}

export type ListStep = { path: string; state: Record<string, unknown> };

export function useListStepping(): { previous: ListStep | null; next: ListStep | null } {
  const navigate = useNavigate();
  const [raw] = useHistoryValue(LIST_CONTEXT_KEY);
  const context = parseListContext(raw);
  const target = (delta: number) => {
    if (context === null) return null;
    const index = context.index + delta;
    return index >= 0 && index < context.paths.length ? index : null;
  };
  const step = (delta: number): ListStep | null => {
    const index = target(delta);
    if (context === null || index === null) return null;
    return { path: context.paths[index], state: listContextState(context.paths, index) };
  };
  const go = (delta: number) => {
    const next = step(delta);
    if (next !== null) navigate(next.path, { state: next.state });
  };
  useHotkey("[", () => go(-1), { enabled: target(-1) !== null });
  useHotkey("]", () => go(1), { enabled: target(1) !== null });
  return { previous: step(-1), next: step(1) };
}

function parseListContext(raw: string): ListContext | null {
  if (raw === "") return null;
  const value: unknown = JSON.parse(raw);
  if (typeof value !== "object" || value === null || !("paths" in value) || !("index" in value)) return null;
  const { paths, index } = value;
  if (!Array.isArray(paths) || !paths.every((path) => typeof path === "string") || typeof index !== "number") return null;
  return { paths, index };
}
