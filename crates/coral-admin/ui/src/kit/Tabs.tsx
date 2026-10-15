import { useRef, type KeyboardEvent, type ReactNode } from "react";

import { formatNumber } from "../lib/format";
import { cx } from "../lib/cx";

export type TabItem<V extends string> = { value: V; label: ReactNode; count?: number | null };

type TabsProps<V extends string> = {
  label: string;
  value: V;
  tabs: readonly TabItem<V>[];
  onChange: (value: V) => void;
  idPrefix?: string;
};

const NEXT_KEYS = new Set(["ArrowRight", "ArrowDown"]);
const PREVIOUS_KEYS = new Set(["ArrowLeft", "ArrowUp"]);

export function Tabs<V extends string>({ label, value, tabs, onChange, idPrefix }: TabsProps<V>) {
  const listRef = useRef<HTMLDivElement>(null);

  const focusTab = (index: number) => {
    const tab = tabs[(index + tabs.length) % tabs.length];
    onChange(tab.value);
    listRef.current?.querySelector<HTMLButtonElement>(`[data-tab="${tab.value}"]`)?.focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const current = tabs.findIndex((tab) => tab.value === value);
    if (NEXT_KEYS.has(event.key)) focusTab(current + 1);
    else if (PREVIOUS_KEYS.has(event.key)) focusTab(current - 1);
    else if (event.key === "Home") focusTab(0);
    else if (event.key === "End") focusTab(tabs.length - 1);
    else return;
    event.preventDefault();
  };

  return (
    <div
      ref={listRef}
      role="tablist"
      aria-label={label}
      onKeyDown={onKeyDown}
      className="flex gap-0.5 overflow-x-auto border-b border-border [scrollbar-width:none]"
    >
      {tabs.map((tab) => {
        const selected = tab.value === value;
        return (
          <button
            key={tab.value}
            type="button"
            role="tab"
            data-tab={tab.value}
            id={idPrefix === undefined ? undefined : `${idPrefix}-tab-${tab.value}`}
            aria-controls={idPrefix === undefined ? undefined : `${idPrefix}-panel`}
            aria-selected={selected}
            tabIndex={selected ? 0 : -1}
            onClick={() => onChange(tab.value)}
            className={cx(
              "-mb-px h-[34px] border-b-2 px-3 text-13 font-medium whitespace-nowrap transition-colors",
              selected ? "border-accent text-fg" : "border-transparent text-muted hover:text-fg",
            )}
          >
            {tab.label}
            {tab.count !== undefined && tab.count !== null && <span className="ml-1.5 text-subtle">{formatNumber(tab.count)}</span>}
          </button>
        );
      })}
    </div>
  );
}
