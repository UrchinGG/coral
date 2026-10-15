import { EntityLink } from "../../kit/EntityLink";
import { Section } from "../../kit/Section";
import { EmptyState } from "../../kit/States";
import { refIdentity } from "../../lib/refs";
import { useRecent } from "../../omnibox/recent";

export function RecentlyViewed() {
  const recent = useRecent();
  return (
    <Section title="Recently viewed" flush>
      {recent.length === 0 ? (
        <EmptyState>Entities you open appear here, newest first.</EmptyState>
      ) : (
        <div className="flex flex-wrap gap-2 px-3.5 py-3">
          {recent.map((ref) => (
            <span key={refIdentity(ref)} className="inline-flex items-center rounded-full border border-border-strong bg-surface-2 px-2.5 py-0.5 text-12">
              <EntityLink entity={ref} />
            </span>
          ))}
        </div>
      )}
    </Section>
  );
}
