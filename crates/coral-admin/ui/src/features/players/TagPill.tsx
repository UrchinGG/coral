import type { TagView } from "../../api/types/players";

type TagPillProps = { tag: Pick<TagView, "tag_type" | "display_name" | "color"> };

const NEUTRAL_TAG_COLOR = "var(--color-muted)";

export function TagPill({ tag }: TagPillProps) {
  const color = tag.color ?? NEUTRAL_TAG_COLOR;
  return (
    <span
      title={tag.tag_type}
      style={{ color, borderColor: color }}
      className="inline-flex h-5 items-center gap-1.5 rounded-full border bg-surface-2 px-[7px] text-11 font-semibold whitespace-nowrap"
    >
      <span aria-hidden className="size-1.5 shrink-0 rounded-full" style={{ backgroundColor: color }} />
      {tag.display_name}
    </span>
  );
}
