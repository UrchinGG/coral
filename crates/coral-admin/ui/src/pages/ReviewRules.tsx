import { useState } from "react";
import { useReviewRules, useSaveReviewRules, type ReviewRules } from "../api/reviewRules";
import { ConfirmButton } from "../components/ConfirmButton";
import { Panel } from "../components/Panel";

const MAX_VALUE = 1000;

type RuleField = {
  key: keyof ReviewRules;
  label: string;
  description: string;
};

const VOTING_FIELDS: RuleField[] = [
  {
    key: "accept_threshold",
    label: "Accepts to approve",
    description: "Accept votes needed, with no rejects or revises, to apply the tag.",
  },
  {
    key: "reject_threshold",
    label: "Rejects to reject",
    description: "Reject votes needed, with no accepts, to close the review without a tag.",
  },
];

const STANDING_FIELDS: RuleField[] = [
  {
    key: "vote_grant_approvals",
    label: "Accepted submissions to vote",
    description: "Accepted submissions needed before a member can vote.",
  },
  {
    key: "tag_grant_correct",
    label: "Correct votes to tag directly",
    description: "Correct votes needed before a member can tag without review.",
  },
  {
    key: "earn_ratio",
    label: "Earn ratio",
    description: "Each rejected submission or incorrect vote raises the target to this many times the misses.",
  },
  {
    key: "bonus_weight",
    label: "Staff-resolved vote weight",
    description: "How many correct votes one correct vote on a staff-resolved review counts as.",
  },
  {
    key: "revoke_strikes",
    label: "Strikes to suspend",
    description: "Active strikes that suspend voting and tagging. Any active strike blocks new grants.",
  },
];

export function ReviewRulesPanel() {
  const query = useReviewRules();
  if (!query.data) {
    return (
      <Panel title="Review rules">
        <div className="text-sm text-gray-500">{query.isLoading ? "Loading…" : "Review rules unavailable."}</div>
      </Panel>
    );
  }
  return <ReviewRulesForm saved={query.data.rules} defaults={query.data.defaults} />;
}

function ReviewRulesForm({ saved, defaults }: { saved: ReviewRules; defaults: ReviewRules }) {
  const [rules, setRules] = useState<ReviewRules>(saved);
  const save = useSaveReviewRules();

  const fields = [...VOTING_FIELDS, ...STANDING_FIELDS];
  const dirty = fields.some(({ key }) => rules[key] !== saved[key]);
  const valid = fields.every(({ key }) => Number.isInteger(rules[key]) && rules[key] >= 1 && rules[key] <= MAX_VALUE);
  const result = save.data;

  const setField = (key: keyof ReviewRules, value: string) => setRules({ ...rules, [key]: Number(value) });

  return (
    <Panel
      title="Review rules"
      description="Open reviews pick up new thresholds on their next vote. Saving re-checks every member's standing; granted permissions are only removed by suspension."
    >
      <div className="flex flex-col gap-4">
        <RuleGroup title="Voting" fields={VOTING_FIELDS} rules={rules} defaults={defaults} onChange={setField} />
        <RuleGroup title="Standing" fields={STANDING_FIELDS} rules={rules} defaults={defaults} onChange={setField} />
        {!valid && <div className="text-xs text-warning">Every value must be a whole number from 1 to {MAX_VALUE}.</div>}
        {result && !dirty && (
          <div className="text-xs text-gray-500">
            Last save changed {result.vote_changes} voting and {result.tag_changes} tagging permissions.
          </div>
        )}
        <div className="flex items-center gap-2">
          <ConfirmButton
            label="Save rules"
            confirmLabel="Apply to all members"
            onConfirm={() => save.mutate(rules)}
            pending={save.isPending}
            disabled={!dirty || !valid}
          />
          <button
            disabled={!dirty}
            onClick={() => setRules(saved)}
            className="rounded-md px-3 py-1 text-xs font-medium text-gray-400 hover:bg-white/5 disabled:opacity-40"
          >
            Discard
          </button>
        </div>
      </div>
    </Panel>
  );
}

function RuleGroup({
  title,
  fields,
  rules,
  defaults,
  onChange,
}: {
  title: string;
  fields: RuleField[];
  rules: ReviewRules;
  defaults: ReviewRules;
  onChange: (key: keyof ReviewRules, value: string) => void;
}) {
  return (
    <div className="flex flex-col gap-3">
      <div className="text-xs font-medium tracking-wide text-gray-400 uppercase">{title}</div>
      {fields.map(({ key, label, description }) => (
        <label key={key} className="flex items-start justify-between gap-3">
          <div>
            <div className="text-sm text-gray-200">{label}</div>
            <div className="text-xs text-gray-500">
              {description} Default {defaults[key]}.
            </div>
          </div>
          <input
            type="number"
            min={1}
            max={MAX_VALUE}
            value={rules[key] || ""}
            onChange={(e) => onChange(key, e.target.value)}
            className="w-20 shrink-0 rounded-md border border-white/10 bg-black/30 px-2 py-1 text-right text-sm"
          />
        </label>
      ))}
    </div>
  );
}
