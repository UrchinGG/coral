import { useEffect, useEffectEvent, type RefObject } from "react";

import { OmniboxList } from "./OmniboxList";
import { useOmniboxKeyboard, useOmniboxNavigation, useOmniboxResults, type PageJump } from "./useOmnibox";

type OmniboxPanelProps = {
  inputRef: RefObject<HTMLInputElement | null>;
  listId: string;
  text: string;
  pages: readonly PageJump[];
  onClose: () => void;
};

export function OmniboxPanel({ inputRef, listId, text, pages, onClose }: OmniboxPanelProps) {
  const results = useOmniboxResults(text, pages);
  const choose = useOmniboxNavigation(onClose);
  const keyboard = useOmniboxKeyboard(results, choose);
  const onKeyDown = useEffectEvent((event: KeyboardEvent) => keyboard.onKeyDown(event));
  const activeId = results.items.length > 0 ? `${listId}-${keyboard.active}` : null;

  useEffect(() => {
    const input = inputRef.current;
    if (input === null) return;
    const listener = (event: KeyboardEvent) => onKeyDown(event);
    input.addEventListener("keydown", listener);
    return () => input.removeEventListener("keydown", listener);
  }, [inputRef]);

  useEffect(() => {
    const input = inputRef.current;
    if (input === null) return;
    if (activeId === null) input.removeAttribute("aria-activedescendant");
    else input.setAttribute("aria-activedescendant", activeId);
    return () => input.removeAttribute("aria-activedescendant");
  }, [inputRef, activeId]);

  return (
    <div className="absolute top-[calc(100%+6px)] left-0 z-[65] flex max-h-[70vh] w-full min-w-[min(520px,calc(100vw-32px))] flex-col overflow-hidden rounded-[10px] border border-border-strong bg-overlay shadow-[0_24px_64px_rgba(0,0,0,0.55)]">
      {results.hint !== "" && <div className="border-b border-border px-3.5 py-1.5 text-11 text-subtle">{results.hint}</div>}
      <OmniboxList results={results} active={keyboard.active} listId={listId} onHover={keyboard.move} onChoose={choose} />
    </div>
  );
}
