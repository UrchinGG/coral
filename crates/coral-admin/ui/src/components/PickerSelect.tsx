import { useState } from "react";

export type PickerItem = {
  id: string;
  label: string;
  color?: string | null;
  hint?: string;
  disabled?: boolean;
};

type PickerSelectProps = {
  items: PickerItem[];
  value: string | null;
  onChange: (id: string | null) => void;
  placeholder: string;
  prefix?: string;
  clearable?: boolean;
  disabled?: boolean;
};

export function PickerSelect({
  items,
  value,
  onChange,
  placeholder,
  prefix = "",
  clearable = true,
  disabled = false,
}: PickerSelectProps) {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");

  const selected = items.find((item) => item.id === value) ?? null;
  const filtered = items.filter((item) => item.label.toLowerCase().includes(search.toLowerCase()));

  const pick = (id: string | null) => {
    onChange(id);
    setOpen(false);
    setSearch("");
  };

  return (
    <div className="relative w-56">
      <button
        disabled={disabled}
        onClick={() => setOpen(!open)}
        className="flex w-full items-center gap-2 rounded-md border border-white/10 bg-black/30 px-2 py-1.5 text-left text-xs hover:border-white/20 disabled:opacity-40"
      >
        {selected ? (
          <ItemLabel item={selected} prefix={prefix} />
        ) : (
          <span className="text-gray-500">{placeholder}</span>
        )}
        <span className="ml-auto text-gray-500">▾</span>
      </button>

      {open && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => pick(value)} />
          <div className="absolute z-50 mt-1 w-full rounded-md border border-white/10 bg-[#12141c] shadow-lg">
            <input
              autoFocus
              className="w-full border-b border-white/8 bg-transparent px-2 py-1.5 text-xs outline-none"
              placeholder="Search…"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
            <div className="max-h-56 overflow-auto py-1">
              {clearable && value !== null && (
                <button
                  onClick={() => pick(null)}
                  className="block w-full px-2 py-1.5 text-left text-xs text-gray-500 hover:bg-white/5"
                >
                  Clear selection
                </button>
              )}
              {filtered.map((item) => (
                <button
                  key={item.id}
                  disabled={item.disabled}
                  onClick={() => pick(item.id)}
                  className={`flex w-full items-center gap-2 px-2 py-1.5 text-left text-xs ${
                    item.disabled ? "cursor-not-allowed opacity-40" : "hover:bg-white/5"
                  } ${item.id === value ? "bg-accent/10" : ""}`}
                >
                  <ItemLabel item={item} prefix={prefix} />
                  {item.hint && <span className="ml-auto text-[10px] text-gray-500">{item.hint}</span>}
                </button>
              ))}
              {filtered.length === 0 && <div className="px-2 py-1.5 text-xs text-gray-500">No matches</div>}
            </div>
          </div>
        </>
      )}
    </div>
  );
}

function ItemLabel({ item, prefix }: { item: PickerItem; prefix: string }) {
  return (
    <span className="flex min-w-0 items-center gap-1.5">
      {item.color !== undefined && (
        <span
          className="h-2.5 w-2.5 flex-shrink-0 rounded-full"
          style={{ backgroundColor: item.color ?? "#6b7280" }}
        />
      )}
      <span className="truncate text-gray-200">
        {prefix}
        {item.label}
      </span>
    </span>
  );
}
