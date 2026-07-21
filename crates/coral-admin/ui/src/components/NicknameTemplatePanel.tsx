import { useState } from "react";
import { useResetNicknames, useSetNicknameTemplate } from "../api/discord";
import {
  byteLength,
  contextFieldPaths,
  NICKNAME_MAX_LEN,
  renderNickname,
  validateTemplate,
  type JsonValue,
} from "../expr";
import { fmtNum } from "../format";
import { ConfirmButton } from "./ConfirmButton";
import { ExpressionEditor } from "./ExpressionEditor";
import { Panel } from "./Panel";

const DEFAULT_TEMPLATE =
  '{bedwars.bracket_open}{achievements.bedwars_level}{\n' +
  'if stats.Bedwars.active_star == "star_black_open": "✫",\n' +
  'stats.Bedwars.active_star == "star_white_circled": "✪",\n' +
  'stats.Bedwars.active_star == "star_white_outlined": "⚝",\n' +
  'stats.Bedwars.active_star == "star_four_clubs": "✥",\n' +
  'stats.Bedwars.active_star == "star_black_outlined": "✭",\n' +
  'stats.Bedwars.active_star == "star_four_pointed": "✦",\n' +
  'stats.Bedwars.active_star == "star_pinwheel": "✵",\n' +
  'stats.Bedwars.active_star == "star_hollow": "✰",\n' +
  'stats.Bedwars.active_star == "star_nautical": "✯",\n' +
  'achievements.bedwars_level < 1000: "✫",\n' +
  'achievements.bedwars_level < 2000: "✪",\n' +
  'achievements.bedwars_level < 3000: "⚝",\n' +
  'achievements.bedwars_level < 4000: "✥",\n' +
  'else: "✭"\n' +
  '}{bedwars.bracket_close} {displayname} | {discord.name}';

const BASE_FIELDS = [
  "displayname",
  "discord.name",
  "stats.Bedwars.active_star",
  "bedwars.bracket_open",
  "bedwars.bracket_close",
  "achievements.bedwars_level",
  "coral.access",
  "blacklist.tag",
];

type NicknameTemplatePanelProps = {
  template: string | null;
  previewContext: Record<string, unknown> | null;
  memberCount: number;
};

export function NicknameTemplatePanel({ template, previewContext, memberCount }: NicknameTemplatePanelProps) {
  const [draft, setDraft] = useState(template ?? "");
  const [showReference, setShowReference] = useState(false);
  const save = useSetNicknameTemplate();
  const reset = useResetNicknames();

  const trimmed = draft.trim();
  const error = trimmed ? validateTemplate(draft) : null;
  const dirty = draft !== (template ?? "");
  const suggestions = dedupe([...BASE_FIELDS, ...contextFieldPaths((previewContext ?? {}) as JsonValue)]).slice(0, 10);

  return (
    <Panel
      title="Display name format"
      description="Template applied to every linked member's server nickname. {…} evaluates an expression, {..…} marks a segment as truncatable."
      action={
        <button
          onClick={() => setShowReference(!showReference)}
          className="rounded-md border border-white/10 px-2 py-1 text-xs text-gray-400 hover:bg-white/8"
        >
          {showReference ? "Hide syntax reference" : "Syntax reference"}
        </button>
      }
    >
      {showReference && <SyntaxReference />}

      <ExpressionEditor
        value={draft}
        onChange={setDraft}
        mode="template"
        placeholder="No format set — nicknames are left unmanaged"
        error={error}
        suggestions={suggestions}
      />

      <LivePreview template={trimmed} error={error} previewContext={previewContext} />

      <div className="mt-3 flex flex-wrap items-center gap-2">
        {trimmed === "" && !template && (
          <button
            onClick={() => setDraft(DEFAULT_TEMPLATE)}
            className="rounded-md bg-accent/15 px-3 py-1.5 text-xs font-medium text-accent hover:bg-accent/25"
          >
            Insert default template
          </button>
        )}
        <ConfirmButton
          label="Save & apply"
          confirmLabel={`Rename ${fmtNum(memberCount)} members`}
          disabled={!dirty || error !== null || trimmed === ""}
          onConfirm={() => save.mutate(draft)}
          pending={save.isPending}
        />
        {template !== null && (
          <ConfirmButton
            label="Clear format"
            confirmLabel="Clear & sync"
            tone="danger"
            onConfirm={() => save.mutate(null)}
            pending={save.isPending}
          />
        )}
        {template === null && (
          <ConfirmButton
            label="Reset all nicknames"
            confirmLabel={`Clear ${fmtNum(memberCount)} nicknames`}
            tone="danger"
            onConfirm={() => reset.mutate()}
            pending={reset.isPending}
          />
        )}
        {dirty && <span className="text-xs text-gray-500">Unsaved changes</span>}
      </div>
    </Panel>
  );
}

function LivePreview({
  template,
  error,
  previewContext,
}: {
  template: string;
  error: string | null;
  previewContext: Record<string, unknown> | null;
}) {
  if (template === "" || error !== null) return null;

  if (!previewContext) {
    return (
      <div className="mt-3 text-xs text-gray-500">
        Live preview unavailable — link a Minecraft account to your admin user to preview against real data.
      </div>
    );
  }

  const rendered = renderNickname(template, previewContext as JsonValue);
  const bytes = byteLength(rendered.truncated);

  return (
    <div className="mt-3 rounded-md border border-white/8 bg-black/30 p-3">
      <div className="mb-1.5 text-[10px] font-medium tracking-wide text-gray-500 uppercase">
        Live preview — your linked account
      </div>
      <div className="flex items-center gap-2.5">
        <span className="flex h-7 w-7 items-center justify-center rounded-full bg-accent/20 text-[10px] font-semibold text-accent">
          {rendered.truncated.slice(0, 2).toUpperCase() || "?"}
        </span>
        <span className="text-sm font-medium text-gray-100">{rendered.truncated || "(empty)"}</span>
        <span className={`ml-auto text-xs ${bytes >= NICKNAME_MAX_LEN ? "text-warning" : "text-gray-500"}`}>
          {bytes}/{NICKNAME_MAX_LEN}
        </span>
      </div>
      {rendered.wasTruncated && rendered.hasTruncatableSegment && (
        <div className="mt-1.5 text-xs text-gray-500">
          The {"{..}"} segment was trimmed to fit. Full render: <span className="font-mono">{rendered.full}</span>
        </div>
      )}
      {rendered.wasTruncated && !rendered.hasTruncatableSegment && (
        <div className="mt-1.5 text-xs text-warning">
          Over {NICKNAME_MAX_LEN} characters with no {"{..}"} segment — the end gets cut off. Mark a segment as{" "}
          {"{..expr}"} to control what shrinks.
        </div>
      )}
    </div>
  );
}

function SyntaxReference() {
  return (
    <div className="mb-3 rounded-md border border-white/8 bg-black/30 p-3 text-xs text-gray-400">
      <div className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5">
        <code className="text-sky-300">{"{expr}"}</code>
        <span>Evaluate an expression</span>
        <code className="text-sky-300">{"{..expr}"}</code>
        <span>Truncatable segment — shrinks first if the nickname exceeds {NICKNAME_MAX_LEN} characters</span>
        <code className="text-sky-300">{"{expr:.1f}"}</code>
        <span>Format a number with fixed decimals</span>
        <code className="text-sky-300">{'{if x < 10: "a", < 20: "b", else: "c"}'}</code>
        <span>Conditional — follow-up branches may omit the left operand</span>
      </div>
      <div className="mt-2.5 border-t border-white/8 pt-2.5">
        <span className="font-medium text-gray-300">Fields</span> — <code>displayname</code> ·{" "}
        <code>achievements.*</code> · <code>stats.Bedwars.*</code> · <code>discord.name</code> ·{" "}
        <code>blacklist.&lt;tag&gt;</code> · <code>coral.access</code> (0=default, 1=trusted, 2=helper, 3=moderator,
        4=admin, 5=owner)
      </div>
      <div className="mt-1.5">
        <span className="font-medium text-gray-300">Operators</span> — <code>+ - * / %</code> ·{" "}
        <code>== != &lt; &lt;= &gt; &gt;=</code> · <code>and or not</code>
      </div>
    </div>
  );
}

function dedupe(values: string[]): string[] {
  return [...new Set(values)];
}
