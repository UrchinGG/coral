import { Search } from "lucide-react";
import { useEffect, useId } from "react";

import { Kbd } from "../kit/Badges";
import { useModalDialog } from "../kit/modal-stack";
import { closePalette, useOmniboxState } from "./omnibox-store";
import { OmniboxList, PaletteFooter } from "./OmniboxList";
import { useOmniboxKeyboard, useOmniboxNavigation, useOmniboxResults, type PageJump } from "./useOmnibox";

type PaletteProps = {
  text: string;
  onTextChange: (text: string) => void;
  pages: readonly PageJump[];
};

export function Palette(props: PaletteProps) {
  const { paletteOpen } = useOmniboxState();
  const dialogRef = useModalDialog(paletteOpen);

  useEffect(() => {
    if (!paletteOpen) return;
    const input = dialogRef.current?.querySelector("input");
    input?.focus();
    input?.select();
  }, [dialogRef, paletteOpen]);

  return (
    <dialog
      ref={dialogRef}
      aria-label="Search"
      onCancel={(event) => {
        event.preventDefault();
        closePalette();
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) closePalette();
      }}
      className="m-auto mt-[10vh] max-h-[76vh] w-[min(680px,calc(100%-32px))] overflow-hidden rounded-[10px] border border-border-strong bg-overlay p-0 text-fg shadow-[0_24px_64px_rgba(0,0,0,0.55)] max-md:mt-4 max-md:max-h-[calc(100dvh-32px)]"
    >
      {paletteOpen && <PaletteBody {...props} />}
    </dialog>
  );
}

function PaletteBody({ text, onTextChange, pages }: PaletteProps) {
  const listId = useId();
  const results = useOmniboxResults(text, pages);
  const choose = useOmniboxNavigation(closePalette);
  const keyboard = useOmniboxKeyboard(results, choose);

  return (
    <div className="flex max-h-[76vh] flex-col max-md:max-h-[calc(100dvh-32px)]">
      <div className="flex items-center gap-2.5 border-b border-border-strong px-3.5">
        <Search className="size-4 shrink-0 text-subtle" />
        <input
          role="combobox"
          aria-label="Search"
          aria-expanded
          aria-controls={listId}
          aria-autocomplete="list"
          aria-activedescendant={results.items.length > 0 ? `${listId}-${keyboard.active}` : undefined}
          autoComplete="off"
          spellCheck={false}
          placeholder="Player, UUID (either form), Discord id or name, IP, key or prefix, request id"
          value={text}
          onChange={(event) => onTextChange(event.target.value)}
          onKeyDown={(event) => keyboard.onKeyDown(event.nativeEvent)}
          className="h-12 min-w-0 flex-1 bg-transparent text-15 outline-none placeholder:text-subtle"
        />
        {results.hint !== "" && <span className="text-11 whitespace-nowrap text-subtle max-md:hidden">{results.hint}</span>}
        <Kbd>esc</Kbd>
      </div>
      <OmniboxList results={results} active={keyboard.active} listId={listId} onHover={keyboard.move} onChoose={choose} />
      <PaletteFooter />
    </div>
  );
}
