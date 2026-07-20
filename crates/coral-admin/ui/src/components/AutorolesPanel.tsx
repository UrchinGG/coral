import { useState } from "react";
import { useAddAutorole, useRemoveAutorole, useStripRole, useUpdateAutorole } from "../api/discord";
import type { AutoroleRuleView, DiscordRoleView } from "../api/types";
import { contextFieldPaths, evalCondition, highlightExpression, validateCondition, type JsonValue } from "../expr";
import { fmtNum } from "../format";
import { Badge } from "./Badge";
import { ConfirmButton } from "./ConfirmButton";
import { ExpressionEditor } from "./ExpressionEditor";
import { Panel } from "./Panel";
import { PickerSelect } from "./PickerSelect";

const COMPARE_OPS = [">=", ">", "==", "!=", "<", "<="] as const;

const BUILDER_FIELDS = [
  "achievements.bedwars_level",
  "stats.Bedwars.wins_bedwars",
  "stats.Bedwars.final_kills_bedwars",
  "coral.access",
  "blacklist.tag",
  "discord.name",
];

type BuilderRow = {
  joiner: "and" | "or";
  field: string;
  op: (typeof COMPARE_OPS)[number];
  value: string;
};

type AutorolesPanelProps = {
  rules: AutoroleRuleView[];
  roles: DiscordRoleView[];
  previewContext: Record<string, unknown> | null;
  memberCount: number;
};

export function AutorolesPanel({ rules, roles, previewContext, memberCount }: AutorolesPanelProps) {
  return (
    <Panel
      title="Autoroles"
      description="Each rule assigns its role to members whose linked account satisfies the condition, and removes it when it stops matching."
    >
      <div className="flex flex-col divide-y divide-white/5">
        {rules.map((rule) => (
          <RuleRow
            key={rule.id}
            rule={rule}
            role={roles.find((r) => r.id === rule.role_id)}
            previewContext={previewContext}
            memberCount={memberCount}
          />
        ))}
        {rules.length === 0 && <div className="pb-3 text-sm text-gray-500">No autorole rules configured.</div>}
      </div>

      <AddRuleForm rules={rules} roles={roles} previewContext={previewContext} memberCount={memberCount} />
      <StripRoleTool roles={roles} memberCount={memberCount} />
    </Panel>
  );
}

function RuleRow({
  rule,
  role,
  previewContext,
  memberCount,
}: {
  rule: AutoroleRuleView;
  role: DiscordRoleView | undefined;
  previewContext: Record<string, unknown> | null;
  memberCount: number;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(rule.condition);
  const update = useUpdateAutorole();
  const remove = useRemoveAutorole();

  const error = draft.trim() ? validateCondition(draft) : "condition is empty";

  return (
    <div className="py-3 first:pt-0">
      <div className="flex flex-wrap items-center gap-2">
        <RoleChip role={role} fallbackId={rule.role_id} />
        {!editing && <ConditionCode condition={rule.condition} />}
        <MatchBadge condition={rule.condition} previewContext={previewContext} />
        <span className="ml-auto flex items-center gap-1.5">
          <button
            onClick={() => {
              setDraft(rule.condition);
              setEditing(!editing);
            }}
            className="rounded-md border border-white/10 px-2 py-1 text-xs text-gray-300 hover:bg-white/8"
          >
            {editing ? "Close" : "Edit"}
          </button>
          <ConfirmButton
            label="Remove"
            confirmLabel="Remove rule"
            tone="danger"
            onConfirm={() => remove.mutate(rule.id)}
            pending={remove.isPending}
          />
        </span>
      </div>
      {editing && (
        <div className="mt-2">
          <ExpressionEditor
            value={draft}
            onChange={setDraft}
            mode="condition"
            error={draft.trim() ? error : null}
            suggestions={fieldSuggestions(previewContext)}
          />
          <div className="mt-2 flex items-center gap-2">
            <ConfirmButton
              label="Save condition"
              confirmLabel={`Re-evaluate ${fmtNum(memberCount)} members`}
              disabled={error !== null || draft === rule.condition}
              onConfirm={() =>
                update.mutate({ id: rule.id, condition: draft }, { onSuccess: () => setEditing(false) })
              }
              pending={update.isPending}
            />
            <MatchBadge condition={draft} previewContext={previewContext} />
          </div>
        </div>
      )}
    </div>
  );
}

function AddRuleForm({
  rules,
  roles,
  previewContext,
  memberCount,
}: {
  rules: AutoroleRuleView[];
  roles: DiscordRoleView[];
  previewContext: Record<string, unknown> | null;
  memberCount: number;
}) {
  const [roleId, setRoleId] = useState<string | null>(null);
  const [mode, setMode] = useState<"builder" | "expression">("builder");
  const [builderRows, setBuilderRows] = useState<BuilderRow[]>([
    { joiner: "and", field: "achievements.bedwars_level", op: ">=", value: "" },
  ]);
  const [rawDraft, setRawDraft] = useState("");
  const add = useAddAutorole();

  const condition = mode === "builder" ? buildCondition(builderRows) : rawDraft;
  const error = condition.trim() ? validateCondition(condition) : "condition is empty";

  const roleItems = roles.map((role) => {
    const hasRule = rules.some((r) => r.role_id === role.id);
    return {
      id: role.id,
      label: role.name,
      color: role.color,
      disabled: !role.assignable || hasRule,
      hint: hasRule ? "has rule" : role.managed ? "managed" : !role.assignable ? "not manageable" : undefined,
    };
  });

  const resetForm = () => {
    setRoleId(null);
    setRawDraft("");
    setBuilderRows([{ joiner: "and", field: "achievements.bedwars_level", op: ">=", value: "" }]);
  };

  return (
    <div className="mt-1 rounded-md border border-white/8 bg-black/20 p-3">
      <div className="mb-2 flex items-center justify-between">
        <span className="text-xs font-medium text-gray-300">Add a rule</span>
        <div className="flex overflow-hidden rounded-md border border-white/8">
          {(["builder", "expression"] as const).map((m) => (
            <button
              key={m}
              onClick={() => {
                if (m === "expression" && mode === "builder" && error === null) setRawDraft(condition);
                setMode(m);
              }}
              className={`px-2.5 py-1 text-xs font-medium ${m === mode ? "bg-accent/15 text-accent" : "text-gray-400 hover:bg-white/5"}`}
            >
              {m === "builder" ? "Builder" : "Expression"}
            </button>
          ))}
        </div>
      </div>

      <div className="mb-2 flex items-center gap-2">
        <span className="text-xs text-gray-500">Role</span>
        <PickerSelect items={roleItems} value={roleId} onChange={setRoleId} placeholder="Select a role" prefix="@" />
      </div>

      {mode === "builder" ? (
        <ConditionBuilder rows={builderRows} onChange={setBuilderRows} />
      ) : (
        <ExpressionEditor
          value={rawDraft}
          onChange={setRawDraft}
          mode="condition"
          placeholder="achievements.bedwars_level >= 500"
          error={rawDraft.trim() ? error : null}
          suggestions={fieldSuggestions(previewContext)}
        />
      )}

      <div className="mt-2 flex flex-wrap items-center gap-2">
        {mode === "builder" && condition.trim() !== "" && <ConditionCode condition={condition} />}
        <MatchBadge condition={condition} previewContext={previewContext} />
        <span className="ml-auto">
          <ConfirmButton
            label="Add rule"
            confirmLabel={`Evaluate ${fmtNum(memberCount)} members`}
            disabled={roleId === null || error !== null}
            onConfirm={() => add.mutate({ role_id: roleId!, condition }, { onSuccess: resetForm })}
            pending={add.isPending}
          />
        </span>
      </div>
    </div>
  );
}

function ConditionBuilder({ rows, onChange }: { rows: BuilderRow[]; onChange: (rows: BuilderRow[]) => void }) {
  const setRow = (index: number, patch: Partial<BuilderRow>) => {
    onChange(rows.map((row, i) => (i === index ? { ...row, ...patch } : row)));
  };

  return (
    <div className="flex flex-col gap-1.5">
      {rows.map((row, i) => (
        <div key={i} className="flex flex-wrap items-center gap-1.5">
          {i > 0 ? (
            <select
              value={row.joiner}
              onChange={(e) => setRow(i, { joiner: e.target.value as BuilderRow["joiner"] })}
              className="rounded-md border border-white/10 bg-black/30 px-1.5 py-1 text-xs"
            >
              <option value="and">and</option>
              <option value="or">or</option>
            </select>
          ) : (
            <span className="w-12 text-right text-xs text-gray-500">where</span>
          )}
          <input
            list="autorole-builder-fields"
            className="w-64 rounded-md border border-white/10 bg-black/30 px-2 py-1 font-mono text-xs"
            placeholder="field"
            value={row.field}
            onChange={(e) => setRow(i, { field: e.target.value })}
          />
          <select
            value={row.op}
            onChange={(e) => setRow(i, { op: e.target.value as BuilderRow["op"] })}
            className="rounded-md border border-white/10 bg-black/30 px-1.5 py-1 font-mono text-xs"
          >
            {COMPARE_OPS.map((op) => (
              <option key={op} value={op}>
                {op}
              </option>
            ))}
          </select>
          <input
            className="w-28 rounded-md border border-white/10 bg-black/30 px-2 py-1 font-mono text-xs"
            placeholder="value"
            value={row.value}
            onChange={(e) => setRow(i, { value: e.target.value })}
          />
          {rows.length > 1 && (
            <button
              onClick={() => onChange(rows.filter((_, j) => j !== i))}
              className="text-xs text-gray-500 hover:text-danger"
            >
              ✕
            </button>
          )}
        </div>
      ))}
      <datalist id="autorole-builder-fields">
        {BUILDER_FIELDS.map((f) => (
          <option key={f} value={f} />
        ))}
      </datalist>
      <button
        onClick={() => onChange([...rows, { joiner: "and", field: "", op: ">=", value: "" }])}
        className="w-fit text-xs text-gray-500 hover:text-gray-300"
      >
        + Add condition
      </button>
    </div>
  );
}

function StripRoleTool({ roles, memberCount }: { roles: DiscordRoleView[]; memberCount: number }) {
  const [roleId, setRoleId] = useState<string | null>(null);
  const strip = useStripRole();

  const items = roles
    .filter((role) => role.assignable)
    .map((role) => ({ id: role.id, label: role.name, color: role.color }));

  return (
    <div className="mt-3 flex flex-wrap items-center gap-2 border-t border-white/8 pt-3">
      <div className="mr-auto">
        <div className="text-xs font-medium text-gray-300">Strip a role</div>
        <div className="text-xs text-gray-500">Remove a role from every member, ignoring any rule.</div>
      </div>
      <PickerSelect items={items} value={roleId} onChange={setRoleId} placeholder="Select a role" prefix="@" />
      <ConfirmButton
        label="Strip role"
        confirmLabel={`Strip from ${fmtNum(memberCount)} members`}
        tone="danger"
        disabled={roleId === null}
        onConfirm={() => strip.mutate(roleId!, { onSuccess: () => setRoleId(null) })}
        pending={strip.isPending}
      />
    </div>
  );
}

function RoleChip({ role, fallbackId }: { role: DiscordRoleView | undefined; fallbackId: string }) {
  return (
    <span className="flex items-center gap-1.5 rounded-full bg-white/5 px-2 py-0.5 text-xs font-medium text-gray-200">
      <span className="h-2.5 w-2.5 rounded-full" style={{ backgroundColor: role?.color ?? "#6b7280" }} />
      @{role?.name ?? fallbackId}
    </span>
  );
}

function ConditionCode({ condition }: { condition: string }) {
  return (
    <code className="rounded bg-black/40 px-1.5 py-0.5 font-mono text-xs">
      {highlightExpression(condition).map((segment, i) => (
        <span
          key={i}
          className={
            {
              literal: "text-gray-300",
              brace: "text-accent",
              field: "text-sky-300",
              string: "text-emerald-300",
              number: "text-amber-300",
              keyword: "text-fuchsia-400",
              operator: "text-gray-500",
            }[segment.kind]
          }
        >
          {segment.text}
        </span>
      ))}
    </code>
  );
}

function MatchBadge({
  condition,
  previewContext,
}: {
  condition: string;
  previewContext: Record<string, unknown> | null;
}) {
  if (!previewContext || condition.trim() === "" || validateCondition(condition) !== null) return null;
  let matches: boolean;
  try {
    matches = evalCondition(condition, previewContext as JsonValue);
  } catch {
    return null;
  }
  return <Badge label={matches ? "matches you" : "no match for you"} tone={matches ? "ok" : "default"} />;
}

function buildCondition(rows: BuilderRow[]): string {
  const parts: string[] = [];
  for (const [i, row] of rows.entries()) {
    if (row.field.trim() === "" || row.value.trim() === "") return "";
    const value = Number.isNaN(Number(row.value.trim())) ? `"${row.value.trim()}"` : row.value.trim();
    const clause = `${row.field.trim()} ${row.op} ${value}`;
    parts.push(i === 0 ? clause : `${row.joiner} ${clause}`);
  }
  return parts.join(" ");
}

function fieldSuggestions(previewContext: Record<string, unknown> | null): string[] {
  if (!previewContext) return BUILDER_FIELDS;
  return [...new Set([...BUILDER_FIELDS, ...contextFieldPaths(previewContext as JsonValue)])].slice(0, 10);
}
