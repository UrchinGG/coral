import { call, load } from "@/lib/api";
import { el } from "@/lib/dom";
import { FORMATS } from "@/lib/format";
import type { PingRoles, Reviews } from "@/lib/reviews/common";
import { smallButton } from "@/lib/server/common";

type ReviewRules = Record<RuleKey, number>;
type RuleKey =
  | "accept_threshold"
  | "reject_threshold"
  | "vote_grant_approvals"
  | "tag_grant_correct"
  | "earn_ratio"
  | "bonus_weight"
  | "revoke_strikes";

interface RulesView {
  rules: ReviewRules;
  defaults: ReviewRules;
}

interface RulesUpdate {
  rules: ReviewRules;
  vote_changes: number;
  tag_changes: number;
}

const MAX_RULE_VALUE = 1_000;

const PING_ROWS: [keyof PingRoles, string, string][] = [
  ["review_role_id", "All reviews", "Pinged for every new tag submission."],
  [
    "dispute_role_id",
    "Disputes only",
    "Pinged when votes disagree and a moderator call is needed.",
  ],
  [
    "confirm_role_id",
    "Confirm ping role",
    "Pinged when a review is accepted with media and its tag can be confirmed.",
  ],
];

const RULE_GROUPS: [string, [RuleKey, string, string][]][] = [
  [
    "Voting",
    [
      [
        "accept_threshold",
        "Accepts to approve",
        "Accept votes needed, with no rejects or revises, to apply the tag.",
      ],
      [
        "reject_threshold",
        "Rejects to reject",
        "Reject votes needed, with no accepts, to close the review without a tag.",
      ],
    ],
  ],
  [
    "Standing",
    [
      [
        "vote_grant_approvals",
        "Accepted submissions to vote",
        "Accepted submissions needed before a member can vote.",
      ],
      [
        "tag_grant_correct",
        "Correct votes to tag directly",
        "Correct votes needed before a member can tag without review.",
      ],
      [
        "earn_ratio",
        "Earn ratio",
        "Each rejected submission or incorrect vote raises the target to this many times the misses.",
      ],
      [
        "bonus_weight",
        "Staff-resolved vote weight",
        "How many correct votes one correct vote on a staff-resolved review counts as.",
      ],
      [
        "revoke_strikes",
        "Strikes to suspend",
        "Active strikes that suspend voting and tagging. Any active strike blocks new grants.",
      ],
    ],
  ],
];

export function pingsTab(reviews: Reviews) {
  const saved = reviews.guide.ping_roles;
  return el(
    "section",
    { className: "panel wide" },
    el("h2", { className: "panel-title" }, "Review Ping Roles"),
    el(
      "p",
      { className: "muted" },
      "Members opt in and out with the guide's Ping Me For Reviews button. Coral pings these roles from review threads.",
    ),
    ...PING_ROWS.map(([field, label, about]) =>
      pingRow(reviews, saved, field, label, about),
    ),
  );
}

function pingRow(
  reviews: Reviews,
  saved: PingRoles,
  field: keyof PingRoles,
  label: string,
  about: string,
) {
  const select = roleSelect(reviews, saved[field]);
  const actions = el("span", { className: "actions" });
  const optIns = el("p", { className: "muted" });
  const update = () => {
    const chosen = select.value || null;
    const count = chosen ? reviews.population?.role_members[chosen] : undefined;
    optIns.textContent =
      count === undefined ? "" : `${FORMATS.number(count)} members opted in`;
    actions.replaceChildren(
      ...(chosen === saved[field]
        ? []
        : [
            smallButton(
              "Save",
              () => savePings(reviews, { ...saved, [field]: chosen }),
              "button small primary",
            ),
            smallButton("Reset", () => {
              select.value = saved[field] ?? "";
              update();
            }),
          ]),
    );
  };
  select.addEventListener("change", update);
  update();
  return el(
    "div",
    { className: "setting" },
    el(
      "div",
      {},
      el("b", { className: "strong" }, label),
      el("p", { className: "muted" }, about),
      optIns,
    ),
    el("span", { className: "actions" }, select, actions),
  );
}

function roleSelect(reviews: Reviews, value: string | null) {
  const select = el(
    "select",
    { className: "field" },
    el("option", { value: "", textContent: "No role" }),
    ...(reviews.directory?.roles ?? []).map((role) =>
      el("option", {
        value: role.id,
        textContent: `@${role.name}${role.bot_assignable === false ? " (not manageable)" : ""}`,
        disabled: role.bot_assignable === false && role.id !== value,
      }),
    ),
  );
  select.setAttribute("aria-label", "Role");
  select.value = value ?? "";
  return select;
}

async function savePings(reviews: Reviews, roles: PingRoles) {
  try {
    await call("/moderation/guide/ping-roles", "PUT", roles);
    reviews.report("Ping roles saved.");
    reviews.refresh();
  } catch (error) {
    reviews.report((error as Error).message, true);
  }
}

export async function rulesTab(reviews: Reviews) {
  const { rules: saved, defaults } = await load<RulesView>("/moderation/rules");
  const draft: ReviewRules = { ...saved };
  const actions = el("span", { className: "actions" });
  const problem = el("p", { className: "bad" });
  const valid = () =>
    Object.values(draft).every(
      (value) =>
        Number.isInteger(value) && value >= 1 && value <= MAX_RULE_VALUE,
    );
  const update = () => {
    const dirty = (Object.keys(saved) as RuleKey[]).some(
      (key) => draft[key] !== saved[key],
    );
    problem.textContent = valid()
      ? ""
      : `Every value must be a whole number from 1 to ${MAX_RULE_VALUE}.`;
    const save = smallButton(
      "Save and Re-check Members",
      () => saveRules(reviews, draft),
      "button small primary",
    );
    save.disabled = !valid();
    actions.replaceChildren(
      ...(dirty ? [save, smallButton("Discard", () => reviews.refresh())] : []),
    );
  };
  const groups = RULE_GROUPS.map(([title, fields]) =>
    el(
      "section",
      { className: "panel wide" },
      el("h2", { className: "panel-title" }, title),
      ...fields.map(([key, label, about]) =>
        ruleRow(draft, defaults, key, label, about, update),
      ),
    ),
  );
  update();
  return el(
    "div",
    { className: "stack" },
    el(
      "div",
      { className: "notice-row" },
      el(
        "span",
        {},
        "Open reviews pick up new thresholds on their next vote. Saving re-checks every member's standing; granted access is only taken away by suspension.",
      ),
      actions,
    ),
    problem,
    ...groups,
  );
}

function ruleRow(
  draft: ReviewRules,
  defaults: ReviewRules,
  key: RuleKey,
  label: string,
  about: string,
  changed: () => void,
) {
  const input = el("input", {
    className: "field rule-number",
    type: "number",
    min: "1",
    max: String(MAX_RULE_VALUE),
    value: String(draft[key]),
  });
  input.setAttribute("aria-label", label);
  const fallback = el("p", { className: "muted" }, `Default ${defaults[key]}`);
  const mark = () =>
    fallback.classList.toggle("warn", draft[key] !== defaults[key]);
  input.addEventListener("input", () => {
    draft[key] = Number(input.value);
    mark();
    changed();
  });
  mark();
  return el(
    "div",
    { className: "setting" },
    el(
      "div",
      {},
      el("b", { className: "strong" }, label),
      el("p", { className: "muted" }, about),
      fallback,
    ),
    input,
  );
}

async function saveRules(reviews: Reviews, rules: ReviewRules) {
  if (
    !confirm(
      "Save the review rules and re-check every member's voting and tagging access?",
    )
  )
    return;
  try {
    const update = await call<RulesUpdate>("/moderation/rules", "PUT", rules);
    reviews.report(
      `Review rules saved; ${update.vote_changes} voting and ${update.tag_changes} tagging permissions changed.`,
    );
    reviews.refresh();
  } catch (error) {
    reviews.report((error as Error).message, true);
  }
}
