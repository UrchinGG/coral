import { useState } from "react";

import { useAction } from "../../api/mutate";
import { resetNicknames, SERVER_INVALIDATES, setNicknameTemplate } from "../../api/server";
import type { StartedJob } from "../../api/types/platform";
import { Button } from "../../kit/Button";
import { ConfirmButton } from "../../kit/ConfirmButton";
import { Section } from "../../kit/Section";
import { byteLength, contextFieldPaths, NICKNAME_MAX_LEN, renderNickname, validateTemplate, type JsonValue } from "./expr";
import { ExpressionEditor } from "./ExpressionEditor";
import { membersPhrase, nicknamesPhrase, startedText } from "./labels";

type NicknameFormatProps = { template: string | null; previewContext: Record<string, unknown> | null; memberCount: number | undefined };

const SUGGESTION_LIMIT = 10;

const BASE_FIELDS = [
  "displayname",
  "discord.name",
  "stats.Bedwars.active_star",
  "bedwars.bracket_open",
  "bedwars.bracket_close",
  "achievements.bedwars_level",
  "coral.access",
  "blacklist.tag",
] as const;

const STAR_GLYPHS = [
  ["star_black_open", "✫"],
  ["star_white_circled", "✪"],
  ["star_white_outlined", "⚝"],
  ["star_four_clubs", "✥"],
  ["star_black_outlined", "✭"],
  ["star_four_pointed", "✦"],
  ["star_pinwheel", "✵"],
  ["star_hollow", "✰"],
  ["star_nautical", "✯"],
] as const;

const LEVEL_GLYPHS = [
  [1000, "✫"],
  [2000, "✪"],
  [3000, "⚝"],
  [4000, "✥"],
] as const;

const DEFAULT_TEMPLATE = [
  "{bedwars.bracket_open}{achievements.bedwars_level}{",
  `if ${STAR_GLYPHS.map(([star, glyph]) => `stats.Bedwars.active_star == "${star}": "${glyph}"`).join(",\n")},`,
  `${LEVEL_GLYPHS.map(([level, glyph]) => `achievements.bedwars_level < ${level}: "${glyph}"`).join(",\n")},`,
  'else: "✭"',
  "}{bedwars.bracket_close} {displayname} | {discord.name}",
].join("\n");

export function NicknameFormat({ template, previewContext, memberCount }: NicknameFormatProps) {
  const [draft, setDraft] = useState(template ?? "");
  const [showReference, setShowReference] = useState(false);
  const save = useAction({
    run: setNicknameTemplate,
    pending: (next) => (next === null ? "Clearing the display name format…" : "Saving the display name format…"),
    done: (started) => startedText(started, "Display name format saved"),
    invalidates: SERVER_INVALIDATES,
  });
  const reset = useAction<void, StartedJob>({
    run: resetNicknames,
    pending: () => "Queueing a nickname reset…",
    done: (started) => startedText(started, "Nickname reset queued"),
    invalidates: SERVER_INVALIDATES,
  });
  const trimmed = draft.trim();
  const error = trimmed === "" ? null : validateTemplate(draft);
  const dirty = draft !== (template ?? "");
  const suggestions = [...new Set([...BASE_FIELDS, ...contextFieldPaths((previewContext ?? {}) as JsonValue)])].slice(0, SUGGESTION_LIMIT);

  return (
    <Section
      title="Display name format"
      actions={
        <Button size="sm" variant="ghost" onClick={() => setShowReference(!showReference)}>
          {showReference ? "Hide syntax reference" : "Syntax reference"}
        </Button>
      }
    >
      <div className="flex flex-col gap-3">
        <p className="m-0 text-12 text-muted">Template applied to every linked member's server nickname. {"{…}"} evaluates an expression, {"{..…}"} marks a segment as truncatable.</p>
        {showReference && <SyntaxReference />}
        <ExpressionEditor label="Display name format" value={draft} onChange={setDraft} mode="template" placeholder="No format set — nicknames are left unmanaged" error={error} suggestions={suggestions} />
        {trimmed !== "" && error === null && <LivePreview template={trimmed} previewContext={previewContext} />}
        <div className="flex flex-wrap items-center gap-2">
          {trimmed === "" && template === null && (
            <Button variant="primary" onClick={() => setDraft(DEFAULT_TEMPLATE)}>
              Insert default template
            </Button>
          )}
          <ConfirmButton confirmLabel={`Rename ${membersPhrase(memberCount)}`} disabled={!dirty || error !== null || trimmed === ""} pending={save.isPending} onConfirm={() => save.mutate(draft)}>
            Save and apply
          </ConfirmButton>
          {template !== null && (
            <ConfirmButton variant="danger" confirmLabel="Clear and sync" pending={save.isPending} onConfirm={() => save.mutate(null)}>
              Clear format
            </ConfirmButton>
          )}
          {template === null && (
            <ConfirmButton variant="danger" confirmLabel={`Clear ${nicknamesPhrase(memberCount)}`} pending={reset.isPending} onConfirm={() => reset.mutate()}>
              Reset all nicknames
            </ConfirmButton>
          )}
          {dirty && <span className="text-12 text-subtle">Unsaved changes</span>}
        </div>
      </div>
    </Section>
  );
}

function LivePreview({ template, previewContext }: { template: string; previewContext: Record<string, unknown> | null }) {
  if (previewContext === null) {
    return <p className="m-0 text-12 text-muted">Live preview unavailable: link a Minecraft account to your admin user to preview against real data.</p>;
  }
  const rendered = renderNickname(template, previewContext as JsonValue);
  const bytes = byteLength(rendered.truncated);
  return (
    <div className="flex flex-col gap-1.5 rounded-md border border-border bg-bg p-3">
      <span className="text-11 font-semibold tracking-[0.06em] text-subtle uppercase">Live preview: your linked account</span>
      <div className="flex items-center gap-2.5">
        <span className="flex size-7 items-center justify-center rounded-full bg-accent/20 text-11 font-semibold text-accent">{rendered.truncated.slice(0, 2).toUpperCase() || "?"}</span>
        <span className="text-13 font-medium wrap-anywhere">{rendered.truncated || "(empty)"}</span>
        <span className={bytes >= NICKNAME_MAX_LEN ? "ml-auto text-12 text-warning" : "ml-auto text-12 text-subtle"}>
          {bytes}/{NICKNAME_MAX_LEN}
        </span>
      </div>
      {rendered.wasTruncated && rendered.hasTruncatableSegment && (
        <span className="text-12 text-muted">
          The {"{..}"} segment was trimmed to fit. Full render: <span className="font-mono">{rendered.full}</span>
        </span>
      )}
      {rendered.wasTruncated && !rendered.hasTruncatableSegment && (
        <span className="text-12 text-warning">
          Over {NICKNAME_MAX_LEN} characters with no {"{..}"} segment, so the end gets cut off. Mark a segment as {"{..expr}"} to control what shrinks.
        </span>
      )}
    </div>
  );
}

function SyntaxReference() {
  return (
    <div className="flex flex-col gap-2 rounded-md border border-border bg-bg p-3 text-12 text-muted">
      <dl className="m-0 grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1.5">
        <dt className="font-mono text-info">{"{expr}"}</dt>
        <dd className="m-0">Evaluate an expression</dd>
        <dt className="font-mono text-info">{"{..expr}"}</dt>
        <dd className="m-0">Truncatable segment: shrinks first if the nickname exceeds {NICKNAME_MAX_LEN} characters</dd>
        <dt className="font-mono text-info">{"{expr:.1f}"}</dt>
        <dd className="m-0">Format a number with fixed decimals</dd>
        <dt className="font-mono text-info">{'{if x < 10: "a", < 20: "b", else: "c"}'}</dt>
        <dd className="m-0">Conditional; follow-up branches may omit the left operand</dd>
      </dl>
      <span>
        <span className="font-medium text-fg">Fields:</span> <code>displayname</code> · <code>achievements.*</code> · <code>stats.Bedwars.*</code> · <code>discord.name</code> · <code>blacklist.&lt;tag&gt;</code> · <code>coral.access</code> (0 default, 1 trusted, 2 helper, 3 moderator, 4 admin, 5 owner)
      </span>
      <span>
        <span className="font-medium text-fg">Operators:</span> <code>+ - * / %</code> · <code>== != &lt; &lt;= &gt; &gt;=</code> · <code>and or not</code>
      </span>
    </div>
  );
}
