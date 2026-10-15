import { ChevronDown } from "lucide-react";
import { useState } from "react";

import type { DiscordRole } from "../../api/types/platform";
import { CONTROL } from "../../kit/control-styles";
import { Popover } from "../../kit/Popover";
import { cx } from "../../lib/cx";
import type { PickerChoice } from "./choices";

type PickerProps = {
  label: string;
  choices: readonly PickerChoice[] | undefined;
  value: string | null;
  onChange: (id: string | null) => void;
  placeholder: string;
  prefix: "@" | "#";
  clearable?: boolean;
};

const UNKNOWN_COLOR = "var(--color-subtle)";
const UNREAD_CHOICES = "Not read from Discord yet.";

export function DiscordPicker({ label, choices, value, onChange, placeholder, prefix, clearable = true }: PickerProps) {
  const selected = choices?.find((choice) => choice.id === value) ?? null;
  return (
    <Popover
      label={label}
      align="start"
      buttonClassName={cx(CONTROL, "flex w-60 max-w-full items-center gap-2 text-left")}
      panelClassName="w-72 max-w-[calc(100vw-32px)] p-1.5"
      button={
        <>
          {value === null ? <span className="text-subtle">{placeholder}</span> : <ChoiceLabel choice={selected ?? unknownChoice(value)} prefix={prefix} />}
          <ChevronDown className="ml-auto size-3.5 shrink-0 text-subtle" />
        </>
      }
    >
      {(close) => (
        <ChoiceList
          choices={choices}
          value={value}
          prefix={prefix}
          clearable={clearable}
          onPick={(id) => {
            onChange(id);
            close();
          }}
        />
      )}
    </Popover>
  );
}

export function RoleChip({ role, id }: { role: DiscordRole | undefined; id: string }) {
  return (
    <span className="inline-flex max-w-full items-center gap-1.5 rounded-full border border-border-strong bg-surface-2 px-2 py-0.5 text-12 font-medium">
      <span className="size-2.5 shrink-0 rounded-full" style={{ backgroundColor: role?.color ?? UNKNOWN_COLOR }} />
      {role === undefined ? <span className="font-mono">@{id}</span> : <span className="wrap-anywhere">@{role.name}</span>}
    </span>
  );
}

function ChoiceList({ choices, value, prefix, clearable, onPick }: { choices: readonly PickerChoice[] | undefined; value: string | null; prefix: string; clearable: boolean; onPick: (id: string | null) => void }) {
  const [search, setSearch] = useState("");
  const needle = search.trim().toLowerCase();
  const visible = (choices ?? []).filter((choice) => choice.label.toLowerCase().includes(needle) || choice.id.includes(needle));
  return (
    <div className="flex flex-col gap-1">
      <input autoFocus value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Filter" aria-label="Filter" className={cx(CONTROL, "w-full")} />
      <ul className="m-0 flex max-h-64 list-none flex-col overflow-y-auto p-0">
        {clearable && value !== null && (
          <li>
            <button type="button" onClick={() => onPick(null)} className="w-full rounded px-2 py-1.5 text-left text-12 text-muted hover:bg-surface-2">
              Clear selection
            </button>
          </li>
        )}
        {visible.map((choice) => (
          <li key={choice.id}>
            <button
              type="button"
              disabled={choice.disabled}
              onClick={() => onPick(choice.id)}
              className={cx("flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-12 hover:bg-surface-2 disabled:cursor-not-allowed disabled:opacity-45", choice.id === value && "bg-accent/14")}
            >
              <ChoiceLabel choice={choice} prefix={prefix} />
              {choice.hint !== undefined && <span className="ml-auto shrink-0 text-11 text-subtle">{choice.hint}</span>}
            </button>
          </li>
        ))}
        {choices === undefined && <li className="px-2 py-1.5 text-12 text-muted">{UNREAD_CHOICES}</li>}
        {choices !== undefined && visible.length === 0 && <li className="px-2 py-1.5 text-12 text-muted">No matches</li>}
      </ul>
    </div>
  );
}

function ChoiceLabel({ choice, prefix }: { choice: PickerChoice; prefix: string }) {
  return (
    <span className="flex min-w-0 items-center gap-1.5">
      {choice.color !== undefined && <span className="size-2.5 shrink-0 rounded-full" style={{ backgroundColor: choice.color ?? UNKNOWN_COLOR }} />}
      <span className="wrap-anywhere">
        {prefix}
        {choice.label}
      </span>
    </span>
  );
}

function unknownChoice(id: string): PickerChoice {
  return { id, label: id };
}
