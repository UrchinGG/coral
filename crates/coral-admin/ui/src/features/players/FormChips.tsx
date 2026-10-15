import type { PlayerForm } from "../../api/types/players";
import { cx } from "../../lib/cx";
import { formatNumber } from "../../lib/format";
import { FORM_ORDER, formLabel } from "./player-state";

type FormChipsProps = {
  selected: readonly PlayerForm[];
  counts: Partial<Record<PlayerForm, number>> | null;
  currentName: string | null;
  formerNames: readonly string[];
  onToggle: (form: PlayerForm) => void;
};

export function FormChips({ selected, counts, currentName, formerNames, onToggle }: FormChipsProps) {
  return (
    <div role="group" aria-label="Lookup forms" className="flex flex-wrap items-center gap-1.5">
      <span className="mr-1 text-11 font-semibold tracking-[0.06em] text-subtle uppercase">Forms</span>
      {FORM_ORDER.map((form) => {
        const pressed = selected.includes(form);
        return (
          <button
            key={form}
            type="button"
            aria-pressed={pressed}
            onClick={() => onToggle(form)}
            className={cx(
              "inline-flex h-6 items-center gap-1.5 rounded-full border px-2.5 text-12 transition-colors",
              pressed ? "border-accent/45 bg-accent/14 text-fg" : "border-border-strong bg-surface text-muted hover:text-fg",
            )}
          >
            {formLabel(form, currentName, formerNames)}
            <b className="font-semibold tabular-nums">{counts === null ? "…" : formatNumber(counts[form] ?? 0)}</b>
            {form === "display" && !pressed && <span className="text-subtle">off</span>}
          </button>
        );
      })}
    </div>
  );
}
