import { useState } from "react";

import { useAction } from "../../api/mutate";
import { addAutorole, removeAutorole, SERVER_INVALIDATES, stripRole, updateAutorole } from "../../api/server";
import type { AutoroleRule, DiscordRole } from "../../api/types/platform";
import { Pill } from "../../kit/Badges";
import { Button } from "../../kit/Button";
import { ConfirmButton } from "../../kit/ConfirmButton";
import { Section } from "../../kit/Section";
import { Segmented } from "../../kit/Segmented";
import { roleChoices } from "./choices";
import { conditionOf, EMPTY_BUILDER, type BuilderRow } from "./condition";
import { ConditionBuilder } from "./ConditionBuilder";
import { DiscordPicker, RoleChip } from "./DiscordPicker";
import { contextFieldPaths, evalCondition, validateCondition, type JsonValue } from "./expr";
import { ExpressionEditor, HighlightedCode } from "./ExpressionEditor";
import { membersPhrase, startedText } from "./labels";

type Context = Record<string, unknown> | null;

type AutorolesProps = { rules: readonly AutoroleRule[]; roles: readonly DiscordRole[] | undefined; previewContext: Context; memberCount: number | undefined };

type EditorMode = "builder" | "expression";

const SUGGESTION_LIMIT = 10;
const CONDITION_FIELDS = [
  "achievements.bedwars_level",
  "stats.Bedwars.wins_bedwars",
  "stats.Bedwars.final_kills_bedwars",
  "coral.access",
  "blacklist.tag",
  "discord.name",
  "standing.trusted",
] as const;
const EDITOR_MODES = [
  { value: "builder", label: "Builder" },
  { value: "expression", label: "Expression" },
] as const;

export function Autoroles({ rules, roles, previewContext, memberCount }: AutorolesProps) {
  return (
    <Section title="Autoroles" count={rules.length}>
      <div className="flex flex-col gap-4">
        <p className="m-0 text-12 text-muted">Each rule assigns its role to members whose linked account satisfies the condition, and removes it when it stops matching.</p>
        <div className="flex flex-col divide-y divide-border">
          {rules.map((rule) => (
            <RuleRow key={rule.id} rule={rule} role={roles?.find((role) => role.id === rule.role_id)} previewContext={previewContext} memberCount={memberCount} />
          ))}
          {rules.length === 0 && <p className="m-0 pb-3 text-12 text-muted">No autorole rules configured.</p>}
        </div>
        <AddRule rules={rules} roles={roles} previewContext={previewContext} memberCount={memberCount} />
        <StripRole roles={roles} memberCount={memberCount} />
      </div>
    </Section>
  );
}

function fieldSuggestions(previewContext: Context): string[] {
  const fromContext = previewContext === null ? [] : contextFieldPaths(previewContext as JsonValue);
  return [...new Set([...CONDITION_FIELDS, ...fromContext])].slice(0, SUGGESTION_LIMIT);
}

function RuleRow({ rule, role, previewContext, memberCount }: { rule: AutoroleRule; role: DiscordRole | undefined; previewContext: Context; memberCount: number | undefined }) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(rule.condition);
  const roleName = role?.name ?? rule.role_id;
  const update = useAction({
    run: (condition: string) => updateAutorole(rule.id, condition),
    pending: () => `Saving the @${roleName} rule…`,
    done: (started) => startedText(started, `@${roleName} rule updated`),
    invalidates: SERVER_INVALIDATES,
    onDone: () => setEditing(false),
  });
  const remove = useAction<void, void>({
    run: () => removeAutorole(rule.id),
    pending: () => `Removing the @${roleName} rule…`,
    done: () => `@${roleName} rule removed`,
    invalidates: SERVER_INVALIDATES,
  });
  const error = draft.trim() === "" ? "condition is empty" : validateCondition(draft);

  return (
    <div className="flex flex-col gap-2 py-3 first:pt-0">
      <div className="flex flex-wrap items-center gap-2">
        <RoleChip role={role} id={rule.role_id} />
        {!editing && <HighlightedCode text={rule.condition} />}
        <MatchPill condition={rule.condition} previewContext={previewContext} />
        <span className="ml-auto flex items-center gap-1.5">
          <Button
            size="sm"
            onClick={() => {
              setDraft(rule.condition);
              setEditing(!editing);
            }}
          >
            {editing ? "Close" : "Edit"}
          </Button>
          <ConfirmButton size="sm" variant="danger" confirmLabel="Remove rule" pending={remove.isPending} onConfirm={() => remove.mutate()}>
            Remove
          </ConfirmButton>
        </span>
      </div>
      {editing && (
        <div className="flex flex-col gap-2">
          <ExpressionEditor label={`Condition for @${roleName}`} value={draft} onChange={setDraft} mode="condition" error={draft.trim() === "" ? null : error} suggestions={fieldSuggestions(previewContext)} />
          <div className="flex flex-wrap items-center gap-2">
            <ConfirmButton confirmLabel={`Re-evaluate ${membersPhrase(memberCount)}`} disabled={error !== null || draft === rule.condition} pending={update.isPending} onConfirm={() => update.mutate(draft)}>
              Save condition
            </ConfirmButton>
            <MatchPill condition={draft} previewContext={previewContext} />
          </div>
        </div>
      )}
    </div>
  );
}

function AddRule({ rules, roles, previewContext, memberCount }: AutorolesProps) {
  const [roleId, setRoleId] = useState<string | null>(null);
  const [mode, setMode] = useState<EditorMode>("builder");
  const [rows, setRows] = useState<BuilderRow[]>(EMPTY_BUILDER);
  const [expression, setExpression] = useState("");
  const add = useAction({
    run: ({ role, condition }: { role: string; condition: string }) => addAutorole(role, condition),
    pending: () => "Adding the autorole rule…",
    done: (started) => startedText(started, "Autorole rule added"),
    invalidates: SERVER_INVALIDATES,
    onDone: () => {
      setRoleId(null);
      setExpression("");
      setRows(EMPTY_BUILDER);
    },
  });
  const condition = mode === "builder" ? conditionOf(rows) : expression;
  const error = condition.trim() === "" ? "condition is empty" : validateCondition(condition);
  const taken = new Set(rules.map((rule) => rule.role_id));
  const switchMode = (next: EditorMode) => {
    if (next === "expression" && mode === "builder" && error === null) setExpression(condition);
    setMode(next);
  };

  return (
    <div className="flex flex-col gap-2.5 rounded-md border border-border bg-bg/60 p-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <span className="text-12 font-medium">Add a rule</span>
        <Segmented label="Condition editor" value={mode} options={EDITOR_MODES} onChange={switchMode} />
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-12 text-subtle">Role</span>
        <DiscordPicker label="Rule role" choices={roleChoices(roles, { taken })} value={roleId} onChange={setRoleId} placeholder="Select a role" prefix="@" />
      </div>
      {mode === "builder" ? (
        <ConditionBuilder rows={rows} onChange={setRows} fields={CONDITION_FIELDS} />
      ) : (
        <ExpressionEditor label="New rule condition" value={expression} onChange={setExpression} mode="condition" placeholder="achievements.bedwars_level >= 500" error={expression.trim() === "" ? null : error} suggestions={fieldSuggestions(previewContext)} />
      )}
      <div className="flex flex-wrap items-center gap-2">
        {mode === "builder" && condition.trim() !== "" && <HighlightedCode text={condition} />}
        <MatchPill condition={condition} previewContext={previewContext} />
        <span className="ml-auto">
          <ConfirmButton
            confirmLabel={`Evaluate ${membersPhrase(memberCount)}`}
            disabled={roleId === null || error !== null}
            pending={add.isPending}
            onConfirm={() => roleId !== null && add.mutate({ role: roleId, condition })}
          >
            Add rule
          </ConfirmButton>
        </span>
      </div>
    </div>
  );
}

function StripRole({ roles, memberCount }: { roles: readonly DiscordRole[] | undefined; memberCount: number | undefined }) {
  const [roleId, setRoleId] = useState<string | null>(null);
  const strip = useAction({
    run: stripRole,
    pending: () => "Queueing the role strip…",
    done: (started) => startedText(started, "Role strip queued"),
    invalidates: SERVER_INVALIDATES,
    onDone: () => setRoleId(null),
  });
  const assignable = roleChoices(roles?.filter((role) => role.assignable));
  return (
    <div className="flex flex-wrap items-center gap-2 border-t border-border pt-3">
      <div className="mr-auto">
        <div className="text-12 font-medium">Strip a role</div>
        <div className="text-12 text-muted">Remove a role from every member, ignoring any rule.</div>
      </div>
      <DiscordPicker label="Role to strip" choices={assignable} value={roleId} onChange={setRoleId} placeholder="Select a role" prefix="@" />
      <ConfirmButton variant="danger" confirmLabel={`Strip from ${membersPhrase(memberCount)}`} disabled={roleId === null} pending={strip.isPending} onConfirm={() => roleId !== null && strip.mutate(roleId)}>
        Strip role
      </ConfirmButton>
    </div>
  );
}

function MatchPill({ condition, previewContext }: { condition: string; previewContext: Context }) {
  if (previewContext === null || condition.trim() === "" || validateCondition(condition) !== null) return null;
  const matches = safeMatch(condition, previewContext);
  if (matches === null) return null;
  return <Pill tone={matches ? "ok" : "neutral"}>{matches ? "matches you" : "no match for you"}</Pill>;
}

function safeMatch(condition: string, previewContext: Record<string, unknown>): boolean | null {
  try {
    return evalCondition(condition, previewContext as JsonValue);
  } catch {
    return null;
  }
}
