import { useQuery } from "@tanstack/react-query";
import { X } from "lucide-react";
import { useState } from "react";

import { isApiError } from "../api/http";
import { searchQuery } from "../api/search";
import type { EntityKind, EntityRef } from "../api/types/refs";
import type { SearchHit } from "../api/types/search";
import { useDebouncedValue } from "../lib/debounce";
import { ENTITY_KIND_LABEL, refFallbackText, refIdentity } from "../lib/refs";
import { Badge } from "./Badges";
import { IconButton } from "./Button";
import { Combobox, type ComboboxOption } from "./Combobox";
import { EntityLabel, EntityLink } from "./EntityLink";
import { FIELD_LABEL } from "./control-styles";

export type EntityPickerValue = { kind: "entity"; ref: EntityRef } | { kind: "raw"; raw: string };

type EntityPickerProps = {
  label: string;
  value: EntityPickerValue | null;
  onChange: (value: EntityPickerValue | null) => void;
  kinds?: readonly EntityKind[];
  operator?: string;
  allowRaw?: boolean;
  rawHint?: string;
  placeholder?: string;
};

const PICKER_DEBOUNCE_MS = 120;
const NOT_FOUND = 404;

export function EntityPicker({ label, value, onChange, kinds, operator = "", allowRaw = false, rawHint, placeholder }: EntityPickerProps) {
  const [text, setText] = useState("");
  const settled = useDebouncedValue(text.trim(), PICKER_DEBOUNCE_MS);
  const search = useQuery({ ...searchQuery(settled === "" ? "" : `${operator}${settled}`), enabled: value === null && settled !== "" });

  if (value !== null) return <PickedValue label={label} value={value} onClear={() => onChange(null)} />;

  const hits = (search.data?.hits ?? []).filter((hit) => kinds === undefined || kinds.includes(hit.ref.kind));
  const options: ComboboxOption<EntityPickerValue>[] = hits.map(hitOption);
  const trimmed = text.trim();
  if (allowRaw && trimmed !== "") {
    options.push({
      key: `raw:${trimmed}`,
      value: { kind: "raw", raw: trimmed },
      text: trimmed,
      label: (
        <span>
          Use raw value <span className="font-mono">{trimmed}</span>
        </span>
      ),
      hint: rawHint,
    });
  }

  return (
    <Combobox
      label={label}
      placeholder={placeholder}
      options={options}
      query={text}
      onQueryChange={setText}
      filter={false}
      onSelect={(option) => {
        setText("");
        onChange(option.value);
      }}
      status={statusOf(trimmed, settled, search.isFetching, search.error)}
      emptyText={trimmed === "" ? "Type to search" : "No matches"}
    />
  );
}

function PickedValue({ label, value, onClear }: { label: string; value: EntityPickerValue; onClear: () => void }) {
  return (
    <div className="flex min-w-0 flex-col gap-1">
      <span className={FIELD_LABEL}>{label}</span>
      <div className="flex min-h-7 min-w-0 items-center gap-1.5 rounded-md border border-border-strong bg-surface-2 py-0.5 pr-0.5 pl-2 text-12">
        {value.kind === "entity" ? (
          <EntityLink entity={value.ref} />
        ) : (
          <>
            <span className="min-w-0 font-mono break-all">{value.raw}</span>
            <Badge>raw</Badge>
          </>
        )}
        <IconButton label={`Clear ${label.toLowerCase()}`} size="sm" className="ml-auto" onClick={onClear}>
          <X className="size-3" />
        </IconButton>
      </div>
    </div>
  );
}

function hitOption(hit: SearchHit): ComboboxOption<EntityPickerValue> {
  const fallback = refFallbackText(hit.ref);
  return {
    key: refIdentity(hit.ref),
    value: { kind: "entity", ref: hit.ref },
    text: fallback.text,
    label: <EntityLabel entity={hit.ref} />,
    hint: `${ENTITY_KIND_LABEL[hit.ref.kind]} · ${hit.reason}`,
  };
}

function statusOf(typed: string, settled: string, fetching: boolean, error: Error | null): string | undefined {
  if (typed === "") return undefined;
  if (fetching || typed !== settled) return "Searching…";
  if (error === null) return undefined;
  if (isApiError(error) && error.status === NOT_FOUND) return "Search is not available on this server yet; pick the raw value.";
  return `Search failed: ${error.message}`;
}
