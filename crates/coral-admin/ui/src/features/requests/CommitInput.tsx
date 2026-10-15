import { useState, type KeyboardEvent } from "react";

import { Input } from "../../kit/Input";

type CommitInputProps = {
  label: string;
  value: string;
  onCommit: (value: string) => void;
  placeholder?: string;
  mono?: boolean;
  error?: string | null;
  className?: string;
};

export function CommitInput({ label, value, onCommit, placeholder, mono = false, error = null, className }: CommitInputProps) {
  const [draft, setDraft] = useState({ base: value, text: value });
  const current = draft.base === value ? draft : { base: value, text: value };
  if (current !== draft) setDraft(current);

  const commit = () => {
    if (current.text !== value) onCommit(current.text.trim());
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") commit();
    if (event.key === "Escape") setDraft({ base: value, text: value });
  };

  return (
    <Input
      label={label}
      value={current.text}
      placeholder={placeholder}
      mono={mono}
      error={error ?? undefined}
      className={className}
      onChange={(event) => setDraft({ base: value, text: event.target.value })}
      onBlur={commit}
      onKeyDown={onKeyDown}
    />
  );
}
