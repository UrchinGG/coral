import { Search } from "lucide-react";
import { lazy, Suspense, useEffect, useId, useRef, useState } from "react";

import { Kbd } from "../kit/Badges";
import { matchesMedia, PHONE_QUERY } from "../lib/media";
import { loadOmniboxPanel } from "./lazy";
import { openPalette, useOmniboxState } from "./omnibox-store";
import type { PageJump } from "./useOmnibox";

type OmniboxProps = {
  text: string;
  onTextChange: (text: string) => void;
  pages: readonly PageJump[];
};

const PLACEHOLDER = "Search player, UUID, account, IP, key, request id…";

const OmniboxPanel = lazy(loadOmniboxPanel);

export function Omnibox({ text, onTextChange, pages }: OmniboxProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const listId = useId();
  const [open, setOpen] = useState(false);
  const { focusRequest } = useOmniboxState();
  const close = () => {
    setOpen(false);
    inputRef.current?.blur();
  };

  useEffect(() => {
    if (focusRequest > 0) inputRef.current?.focus();
  }, [focusRequest]);

  return (
    <div className="relative max-w-[620px] min-w-0 flex-auto">
      <label className="flex h-8 items-center gap-2 rounded-md border border-border bg-bg pr-1.5 pl-2.5 text-subtle transition-colors focus-within:border-accent hover:border-border-strong">
        <Search className="size-3.5 shrink-0" />
        <input
          ref={inputRef}
          role="combobox"
          aria-label="Search"
          aria-expanded={open}
          aria-controls={listId}
          aria-autocomplete="list"
          autoComplete="off"
          spellCheck={false}
          placeholder={PLACEHOLDER}
          value={text}
          onChange={(event) => {
            onTextChange(event.target.value);
            setOpen(true);
          }}
          onFocus={(event) => {
            if (matchesMedia(PHONE_QUERY)) {
              event.currentTarget.blur();
              openPalette();
              return;
            }
            setOpen(true);
          }}
          onBlur={() => setOpen(false)}
          onKeyDown={(event) => {
            if (event.defaultPrevented) return;
            if (event.key === "Escape") {
              event.preventDefault();
              close();
              return;
            }
            setOpen(true);
          }}
          className="h-full min-w-0 flex-1 bg-transparent text-13 text-fg outline-none placeholder:text-subtle"
        />
        <span className="max-md:hidden">
          <Kbd>⌘K</Kbd>
        </span>
      </label>
      <Suspense>{open && <OmniboxPanel inputRef={inputRef} listId={listId} text={text} pages={pages} onClose={close} />}</Suspense>
    </div>
  );
}
