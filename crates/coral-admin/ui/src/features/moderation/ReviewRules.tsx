import { useQuery } from "@tanstack/react-query";
import { useState } from "react";

import { keys } from "../../api/keys";
import { rulesQuery, saveRules } from "../../api/moderation";
import { useAction } from "../../api/mutate";
import type { ReviewRules } from "../../api/types/platform";
import { Button } from "../../kit/Button";
import { ConfirmButton } from "../../kit/ConfirmButton";
import { CONTROL } from "../../kit/control-styles";
import { QueryContent } from "../../kit/QueryContent";
import { Section } from "../../kit/Section";
import { cx } from "../../lib/cx";

type RuleField = { key: keyof ReviewRules; label: string; description: string };

const MAX_RULE_VALUE = 1_000;

const VOTING_FIELDS: readonly RuleField[] = [
  { key: "accept_threshold", label: "Accepts to approve", description: "Accept votes needed, with no rejects or revises, to apply the tag." },
  { key: "reject_threshold", label: "Rejects to reject", description: "Reject votes needed, with no accepts, to close the review without a tag." },
];

const STANDING_FIELDS: readonly RuleField[] = [
  { key: "vote_grant_approvals", label: "Accepted submissions to vote", description: "Accepted submissions needed before a member can vote." },
  { key: "tag_grant_correct", label: "Correct votes to tag directly", description: "Correct votes needed before a member can tag without review." },
  { key: "earn_ratio", label: "Earn ratio", description: "Each rejected submission or incorrect vote raises the target to this many times the misses." },
  { key: "bonus_weight", label: "Staff-resolved vote weight", description: "How many correct votes one correct vote on a staff-resolved review counts as." },
  { key: "revoke_strikes", label: "Strikes to suspend", description: "Active strikes that suspend voting and tagging. Any active strike blocks new grants." },
];

const ALL_FIELDS = [...VOTING_FIELDS, ...STANDING_FIELDS];

export function ReviewRulesPanel() {
  const rules = useQuery(rulesQuery());
  return (
    <Section title="Review rules" fetching={rules.isFetching && rules.data !== undefined}>
      <QueryContent query={rules}>{(data) => <RulesForm saved={data.rules} defaults={data.defaults} />}</QueryContent>
    </Section>
  );
}

function RulesForm({ saved, defaults }: { saved: ReviewRules; defaults: ReviewRules }) {
  const [rules, setRules] = useState<ReviewRules>(saved);
  const [outcome, setOutcome] = useState<string | null>(null);
  const save = useAction({
    run: saveRules,
    pending: () => "Saving the rules and re-checking every member's standing…",
    done: (update) => `Review rules saved; ${update.vote_changes} voting and ${update.tag_changes} tagging permissions changed`,
    invalidates: [keys.moderation.rules()],
    onDone: (update) => {
      setRules(update.rules);
      setOutcome(`Last save changed ${update.vote_changes} voting and ${update.tag_changes} tagging permissions.`);
    },
  });
  const dirty = ALL_FIELDS.some(({ key }) => rules[key] !== saved[key]);
  const valid = ALL_FIELDS.every(({ key }) => Number.isInteger(rules[key]) && rules[key] >= 1 && rules[key] <= MAX_RULE_VALUE);
  const setField = (key: keyof ReviewRules, value: string) => setRules({ ...rules, [key]: Number(value) });

  return (
    <div className="flex flex-col gap-4">
      <p className="m-0 text-12 text-muted">Open reviews pick up new thresholds on their next vote. Saving re-checks every member's standing; granted permissions are only removed by suspension.</p>
      <RuleGroup title="Voting" fields={VOTING_FIELDS} rules={rules} defaults={defaults} onChange={setField} />
      <RuleGroup title="Standing" fields={STANDING_FIELDS} rules={rules} defaults={defaults} onChange={setField} />
      {!valid && <span className="text-12 text-warning">Every value must be a whole number from 1 to {MAX_RULE_VALUE}.</span>}
      {outcome !== null && !dirty && <span className="text-12 text-muted">{outcome}</span>}
      <div className="flex items-center gap-2">
        <ConfirmButton confirmLabel="Apply to all members" disabled={!dirty || !valid} pending={save.isPending} onConfirm={() => save.mutate(rules)}>
          Save rules
        </ConfirmButton>
        <Button variant="ghost" disabled={!dirty} onClick={() => setRules(saved)}>
          Discard
        </Button>
      </div>
    </div>
  );
}

function RuleGroup({ title, fields, rules, defaults, onChange }: { title: string; fields: readonly RuleField[]; rules: ReviewRules; defaults: ReviewRules; onChange: (key: keyof ReviewRules, value: string) => void }) {
  return (
    <div className="flex flex-col gap-3">
      <span className="text-11 font-semibold tracking-[0.06em] text-subtle uppercase">{title}</span>
      {fields.map(({ key, label, description }) => (
        <label key={key} className="flex items-start justify-between gap-3">
          <span className="flex flex-col">
            <span className="text-13">{label}</span>
            <span className="text-12 text-muted">
              {description} Default {defaults[key]}.
            </span>
          </span>
          <input type="number" min={1} max={MAX_RULE_VALUE} value={rules[key] || ""} onChange={(event) => onChange(key, event.target.value)} className={cx(CONTROL, "w-20 shrink-0 text-right")} />
        </label>
      ))}
    </div>
  );
}
