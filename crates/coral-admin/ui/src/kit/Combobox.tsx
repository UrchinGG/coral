import { useId, useState, type KeyboardEvent, type ReactNode } from "react";

import { cx } from "../lib/cx";
import { CONTROL } from "./control-styles";
import { Field } from "./Input";

export type ComboboxOption<V> = {
  key: string;
  value: V;
  text: string;
  label?: ReactNode;
  hint?: ReactNode;
};

type ComboboxProps<V> = {
  label?: string;
  placeholder?: string;
  mono?: boolean;
  options: readonly ComboboxOption<V>[];
  query: string;
  onQueryChange: (query: string) => void;
  onSelect: (option: ComboboxOption<V>) => void;
  filter?: boolean;
  status?: ReactNode;
  emptyText?: string;
  className?: string;
};

type ComboboxListProps<V> = {
  listId: string;
  options: readonly ComboboxOption<V>[];
  activeIndex: number;
  onActivate: (index: number) => void;
  onChoose: (option: ComboboxOption<V>) => void;
  status: ReactNode;
  emptyText: string;
};

export function Combobox<V>(props: ComboboxProps<V>) {
  const { label, placeholder, mono = false, query, status, emptyText = "No matches", className } = props;
  const inputId = useId();
  const listId = useId();
  const box = useComboboxState(props);

  const input = (
    <div className={cx("relative min-w-0", className)}>
      <input
        id={inputId}
        role="combobox"
        aria-expanded={box.open}
        aria-controls={listId}
        aria-autocomplete="list"
        aria-activedescendant={box.open && box.visible.length > 0 ? `${listId}-${box.activeIndex}` : undefined}
        autoComplete="off"
        spellCheck={false}
        placeholder={placeholder}
        value={query}
        onChange={(event) => box.type(event.target.value)}
        onFocus={() => box.setOpen(true)}
        onBlur={() => box.setOpen(false)}
        onKeyDown={box.onKeyDown}
        className={cx(CONTROL, "w-full", mono && "font-mono")}
      />
      {box.open && <ComboboxList listId={listId} options={box.visible} activeIndex={box.activeIndex} onActivate={box.setActive} onChoose={box.choose} status={status} emptyText={emptyText} />}
    </div>
  );
  if (label === undefined) return input;
  return (
    <Field label={label} htmlFor={inputId}>
      {input}
    </Field>
  );
}

function useComboboxState<V>({ options, query, onQueryChange, onSelect, filter = true }: ComboboxProps<V>) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const needle = query.trim().toLowerCase();
  const visible = filter ? options.filter((option) => option.text.toLowerCase().includes(needle)) : options;
  const activeIndex = Math.min(active, Math.max(visible.length - 1, 0));

  const choose = (option: ComboboxOption<V> | undefined) => {
    if (option === undefined) return;
    onSelect(option);
    setOpen(false);
  };
  const type = (text: string) => {
    onQueryChange(text);
    setActive(0);
    setOpen(true);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      setOpen(true);
      if (visible.length > 0) setActive((activeIndex + (event.key === "ArrowDown" ? 1 : -1) + visible.length) % visible.length);
      return;
    }
    if (!open) return;
    if (event.key === "Enter") {
      event.preventDefault();
      choose(visible[activeIndex]);
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      setOpen(false);
    }
  };

  return { open, setOpen, visible, activeIndex, setActive, choose, type, onKeyDown };
}

function ComboboxList<V>({ listId, options, activeIndex, onActivate, onChoose, status, emptyText }: ComboboxListProps<V>) {
  return (
    <div
      onPointerDown={(event) => event.preventDefault()}
      className="absolute top-[calc(100%+4px)] right-0 left-0 z-30 max-h-64 overflow-y-auto rounded-md border border-border-strong bg-overlay p-1 shadow-[0_12px_32px_rgba(0,0,0,0.45)]"
    >
      <ul id={listId} role="listbox" className="m-0 list-none p-0">
        {options.map((option, index) => (
          <li
            key={option.key}
            id={`${listId}-${index}`}
            role="option"
            aria-selected={index === activeIndex}
            onPointerEnter={() => onActivate(index)}
            onClick={() => onChoose(option)}
            className={cx("flex cursor-pointer flex-col gap-0.5 rounded px-2 py-1.5 text-12", index === activeIndex && "bg-accent/14 shadow-[inset_0_0_0_1px_rgba(129,140,248,0.35)]")}
          >
            <span className="wrap-anywhere">{option.label ?? option.text}</span>
            {option.hint !== undefined && <span className="text-11 text-muted">{option.hint}</span>}
          </li>
        ))}
      </ul>
      {options.length === 0 && status === undefined && <div className="px-2 py-1.5 text-12 text-muted">{emptyText}</div>}
      {status !== undefined && <div className="px-2 py-1.5 text-11 text-muted">{status}</div>}
    </div>
  );
}
