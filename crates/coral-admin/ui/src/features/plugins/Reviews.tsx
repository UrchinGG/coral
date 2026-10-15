import { keys } from "../../api/keys";
import { useAction } from "../../api/mutate";
import { deleteReview } from "../../api/plugins";
import type { PluginReview } from "../../api/types/platform";
import { ClampText } from "../../kit/ClampText";
import { ConfirmButton } from "../../kit/ConfirmButton";
import { EntityLink } from "../../kit/EntityLink";
import { Section } from "../../kit/Section";
import { Table, type Column } from "../../kit/Table";
import { When } from "../platform/When";

const STAR = "★";

export function Reviews({ slug, reviews }: { slug: string; reviews: readonly PluginReview[] }) {
  const remove = useAction({
    run: (userId: number) => deleteReview(slug, userId),
    pending: () => "Deleting the review…",
    done: () => "Review deleted",
    invalidates: [keys.plugins.core(slug), keys.plugins.all()],
  });
  const columns: readonly Column<PluginReview>[] = [
    { id: "reviewer", header: "Reviewer", cell: (review) => (review.reviewer === null ? <span className="font-mono text-12">Starfish user {review.user_id}</span> : <EntityLink entity={review.reviewer} />) },
    { id: "stars", header: "Stars", cell: (review) => <span className="text-warning" title={`${review.stars} of 5`}>{STAR.repeat(review.stars)}</span> },
    { id: "review", header: "Review", cell: (review) => (review.review === null ? <span className="text-subtle">no text</span> : <ClampText>{review.review}</ClampText>), className: "min-w-[220px]" },
    { id: "updated", header: "Updated", cell: (review) => <When at={review.updated_at} />, priority: 2 },
    {
      id: "actions",
      header: "",
      cell: (review) => (
        <ConfirmButton size="sm" variant="danger" confirmLabel="Delete review" pending={remove.isPending && remove.variables === review.user_id} onConfirm={() => remove.mutate(review.user_id)}>
          Delete
        </ConfirmButton>
      ),
    },
  ];
  return (
    <Section title="Reviews" count={reviews.length} flush>
      <Table label="Reviews" columns={columns} rows={reviews} rowKey={(review) => String(review.user_id)} empty="Nobody has reviewed this plugin." />
    </Section>
  );
}
