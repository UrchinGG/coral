import type { TagType } from "../../api/types/platform";

type TagBadgeProps = { tag: string; types: readonly TagType[] | undefined };

const UNKNOWN_TAG_COLOR = "var(--color-subtle)";

export function TagBadge({ tag, types }: TagBadgeProps) {
  const type = types?.find((candidate) => candidate.name === tag);
  return (
    <span className="inline-flex items-center gap-1.5 rounded-full border border-border-strong bg-surface-2 px-2 py-0.5 text-11 font-semibold whitespace-nowrap">
      <span className="size-2 shrink-0 rounded-full" style={{ backgroundColor: type?.color ?? UNKNOWN_TAG_COLOR }} />
      {type?.display_name ?? tag}
    </span>
  );
}
