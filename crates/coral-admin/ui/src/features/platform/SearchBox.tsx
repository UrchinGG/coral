import { useState, type FormEvent } from "react";

import { Button } from "../../kit/Button";
import { Input } from "../../kit/Input";

type SearchBoxProps = { label: string; placeholder: string; value: string; onSearch: (value: string | null) => void };

export function SearchBox({ label, placeholder, value, onSearch }: SearchBoxProps) {
  const [draft, setDraft] = useState(value);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    onSearch(draft.trim() === "" ? null : draft.trim());
  };
  return (
    <form role="search" onSubmit={submit} className="flex flex-wrap items-end gap-2">
      <Input aria-label={label} placeholder={placeholder} value={draft} onChange={(event) => setDraft(event.target.value)} className="w-80 max-w-full" />
      <Button type="submit">Search</Button>
      {value !== "" && (
        <Button
          variant="ghost"
          onClick={() => {
            setDraft("");
            onSearch(null);
          }}
        >
          Clear
        </Button>
      )}
    </form>
  );
}
