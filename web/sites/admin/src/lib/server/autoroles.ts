import { el } from "@/lib/dom";
import { expressionEditor, highlighted } from "@/lib/editor";
import { evalCondition, validateCondition, type JsonValue } from "@/lib/expr";
import {
  apply,
  membersPhrase,
  roleChip,
  roleSelect,
  smallButton,
  type Rule,
  type Server,
} from "@/lib/server/common";
import { suggestions } from "@/lib/server/nicknames";

const CONDITION_FIELDS = [
  "hypixel.achievements.bedwars_level",
  "hypixel.stats.Bedwars.wins_bedwars",
  "hypixel.stats.Bedwars.final_kills_bedwars",
  "coral.access",
  "blacklist.tag",
  "discord.name",
  "standing.trusted",
];

export function autorolesTab(server: Server) {
  const rules = server.config.rules;
  return el(
    "div",
    { className: "stack" },
    el(
      "section",
      { className: "panel wide" },
      el("h2", { className: "panel-title" }, `Autoroles · ${rules.length}`),
      el(
        "p",
        { className: "muted" },
        "Each rule gives its role to members whose linked account meets the condition, and takes it away when they stop meeting it.",
      ),
      ...(rules.length
        ? rules.map((rule) => ruleRow(server, rule))
        : [el("p", { className: "muted" }, "No autorole rules yet.")]),
    ),
    addRule(server),
    stripRole(server),
  );
}

function ruleRow(server: Server, rule: Rule) {
  const editing = el("div", { className: "stack", hidden: true });
  const line = el(
    "div",
    { className: "rule" },
    roleChip(server, rule.role_id),
    highlighted(rule.condition),
    matchPill(server, rule.condition),
    el(
      "span",
      { className: "inline push" },
      smallButton("Edit", () => {
        editing.hidden = !editing.hidden;
        if (!editing.hidden) editing.replaceChildren(...editRule(server, rule));
      }),
      smallButton(
        "Remove",
        () =>
          confirm(
            "Remove this rule? The role stays on members until stripped.",
          ) &&
          apply(
            server,
            `/rules/${rule.id}`,
            "DELETE",
            undefined,
            "Rule removed",
          ),
        "button small danger",
      ),
    ),
  );
  return el("div", { className: "rule-block" }, line, editing);
}

function editRule(server: Server, rule: Rule) {
  const condition = conditionField(server, rule.condition, "Condition");
  const save = smallButton(
    "Save condition",
    () =>
      confirm(`Re-evaluate ${membersPhrase(server)}?`) &&
      apply(
        server,
        `/rules/${rule.id}`,
        "PUT",
        { condition: condition.value() },
        "Rule saved",
      ),
    "button primary",
  );
  condition.onChange(
    (valid, value) => (save.disabled = !valid || value === rule.condition),
  );
  return [
    condition.element,
    el("div", { className: "actions" }, save, condition.pill),
  ];
}

function addRule(server: Server) {
  const taken = new Set(server.config.rules.map((rule) => rule.role_id));
  const role = roleSelect(server, null, taken);
  const condition = conditionField(server, "", "New rule condition");
  const add = smallButton(
    "Add rule",
    () =>
      confirm(`Evaluate ${membersPhrase(server)} for this rule?`) &&
      apply(
        server,
        "/rules",
        "POST",
        { role_id: role.value, condition: condition.value() },
        "Rule added",
      ),
    "button primary",
  );
  const sync = () => (add.disabled = !role.value || !condition.valid());
  role.addEventListener("change", sync);
  condition.onChange(sync);
  sync();
  return el(
    "section",
    { className: "panel wide" },
    el("h2", { className: "panel-title" }, "Add a Rule"),
    el(
      "label",
      { className: "inline" },
      el("span", { className: "muted" }, "Role"),
      role,
    ),
    condition.element,
    el("div", { className: "actions" }, add, condition.pill),
  );
}

function stripRole(server: Server) {
  const role = roleSelect(server, null);
  const strip = smallButton(
    "Strip role",
    () =>
      confirm(
        `Remove this role from ${membersPhrase(server)}, ignoring every rule?`,
      ) &&
      apply(
        server,
        `/roles/${role.value}/strip`,
        "POST",
        {},
        "Role strip queued",
      ),
    "button danger",
  );
  const sync = () => (strip.disabled = !role.value);
  role.addEventListener("change", sync);
  sync();
  return el(
    "section",
    { className: "panel wide" },
    el("h2", { className: "panel-title" }, "Strip a Role"),
    el(
      "p",
      { className: "muted" },
      "Removes a role from every member, ignoring any rule.",
    ),
    el("div", { className: "inline" }, role, strip),
  );
}

function conditionField(server: Server, initial: string, label: string) {
  const error = el("p", { className: "bad" });
  const pill = el("span");
  let current = initial;
  let listener: (valid: boolean, value: string) => void = () => {};
  const problem = () =>
    current.trim() ? validateCondition(current) : "The condition is empty";
  const update = () => {
    const found = problem();
    error.textContent = current.trim() ? (found ?? "") : "";
    pill.replaceChildren(matchPill(server, current));
    listener(found === null, current);
  };
  const editor = expressionEditor({
    label,
    value: initial,
    mode: "condition",
    placeholder: "hypixel.achievements.bedwars_level >= 500",
    suggestions: suggestions(server, CONDITION_FIELDS),
    onInput: (value) => {
      current = value;
      update();
    },
  });
  update();
  return {
    element: el("div", { className: "stack" }, editor.element, error),
    pill,
    value: () => current,
    valid: () => problem() === null,
    onChange: (next: (valid: boolean, value: string) => void) => {
      listener = next;
      update();
    },
  };
}

function matchPill(server: Server, condition: string): Node {
  const context = server.config.preview_context;
  if (!context || !condition.trim() || validateCondition(condition) !== null)
    return el("span");
  let matches: boolean;
  try {
    matches = evalCondition(condition, context as JsonValue);
  } catch {
    return el("span");
  }
  return el(
    "span",
    { className: matches ? "pill ok" : "pill dim" },
    matches ? "Matches you" : "Doesn't match you",
  );
}
