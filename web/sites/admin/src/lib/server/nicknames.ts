import { el } from "@/lib/dom";
import { expressionEditor } from "@/lib/editor";
import {
  byteLength,
  contextFieldPaths,
  NICKNAME_MAX_LEN,
  renderNickname,
  validateTemplate,
  type JsonValue,
} from "@/lib/expr";
import {
  apply,
  membersPhrase,
  smallButton,
  type Server,
} from "@/lib/server/common";

const SUGGESTION_LIMIT = 10;

const BASE_FIELDS = [
  "hypixel.displayname",
  "discord.name",
  "hypixel.stats.Bedwars.active_star",
  "bedwars.bracket_open",
  "bedwars.bracket_close",
  "hypixel.achievements.bedwars_level",
  "coral.access",
  "blacklist.tag",
];

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
];

const LEVEL_GLYPHS = [
  [1000, "✫"],
  [2000, "✪"],
  [3000, "⚝"],
  [4000, "✥"],
];

const DEFAULT_TEMPLATE = [
  "{bedwars.bracket_open}{hypixel.achievements.bedwars_level}{",
  `if ${STAR_GLYPHS.map(([star, glyph]) => `hypixel.stats.Bedwars.active_star == "${star}": "${glyph}"`).join(",\n")},`,
  `${LEVEL_GLYPHS.map(([level, glyph]) => `hypixel.achievements.bedwars_level < ${level}: "${glyph}"`).join(",\n")},`,
  'else: "✭"',
  "}{bedwars.bracket_close} {hypixel.displayname} | {discord.name}",
].join("\n");

export function suggestions(server: Server, base: string[]) {
  const context = server.config.preview_context;
  const found = context ? contextFieldPaths(context as JsonValue) : [];
  return [...new Set([...base, ...found])].slice(0, SUGGESTION_LIMIT);
}

export function nicknamesTab(server: Server) {
  const saved = server.config.config.nickname_template;
  const error = el("p", { className: "bad" });
  const preview = el("div", { className: "preview" });
  const actions = el("div", { className: "actions" });
  let draft = saved ?? "";
  const editor = expressionEditor({
    label: "Display name format",
    value: draft,
    mode: "template",
    placeholder: "No format set; nicknames are left alone",
    suggestions: suggestions(server, BASE_FIELDS),
    onInput: (value) => {
      draft = value;
      update();
    },
  });
  const update = () => {
    const trimmed = draft.trim();
    const problem = trimmed ? validateTemplate(draft) : null;
    error.textContent = problem ?? "";
    preview.replaceChildren(
      ...(trimmed && !problem ? livePreview(server, trimmed) : []),
    );
    actions.replaceChildren(
      ...buttons(server, saved, draft, problem, editor.area),
    );
  };
  update();
  return el(
    "section",
    { className: "panel wide" },
    el("h2", { className: "panel-title" }, "Display Name Format"),
    el(
      "p",
      { className: "muted" },
      "Applied to every linked member's server nickname. {expr} evaluates an expression and {..expr} marks the part that shrinks first.",
    ),
    syntaxReference(),
    editor.element,
    error,
    preview,
    actions,
  );
}

function buttons(
  server: Server,
  saved: string | null,
  draft: string,
  problem: string | null,
  area: HTMLTextAreaElement,
) {
  const trimmed = draft.trim();
  const save = smallButton(
    "Save and apply",
    () =>
      confirm(`Rename ${membersPhrase(server)}?`) &&
      apply(
        server,
        "/nickname-template",
        "PUT",
        { template: draft },
        "Display name format saved",
      ),
    "button primary",
  );
  save.disabled = draft === (saved ?? "") || problem !== null || !trimmed;
  const shown = [save];
  if (!trimmed && saved === null)
    shown.unshift(
      smallButton("Insert default", () => {
        area.value = DEFAULT_TEMPLATE;
        area.dispatchEvent(new Event("input"));
      }),
    );
  if (saved !== null)
    shown.push(
      smallButton(
        "Clear format",
        () =>
          confirm("Clear the format and stop managing nicknames?") &&
          apply(
            server,
            "/nickname-template",
            "PUT",
            { template: null },
            "Display name format cleared",
          ),
        "button danger",
      ),
    );
  else
    shown.push(
      smallButton(
        "Reset all nicknames",
        () =>
          confirm(`Clear the nicknames of ${membersPhrase(server)}?`) &&
          apply(
            server,
            "/nicknames/reset",
            "POST",
            {},
            "Nickname reset queued",
          ),
        "button danger",
      ),
    );
  return shown;
}

function livePreview(server: Server, template: string): Node[] {
  const context = server.config.preview_context;
  if (!context)
    return [
      el(
        "p",
        { className: "muted" },
        "No live preview: link a Minecraft account to your own Discord account to preview against real data.",
      ),
    ];
  const rendered = renderNickname(template, context as JsonValue);
  const bytes = byteLength(rendered.truncated);
  const notes: Node[] = [];
  if (rendered.wasTruncated && rendered.hasTruncatableSegment)
    notes.push(
      el(
        "p",
        { className: "muted" },
        "The {..} part was shortened to fit. In full: ",
        el("span", { className: "mono" }, rendered.full),
      ),
    );
  if (rendered.wasTruncated && !rendered.hasTruncatableSegment)
    notes.push(
      el(
        "p",
        { className: "warn" },
        `Over ${NICKNAME_MAX_LEN} characters with no {..} part, so the end gets cut off.`,
      ),
    );
  return [
    el("span", { className: "muted" }, "Preview, your linked account"),
    el(
      "div",
      { className: "preview-name" },
      el("b", {}, rendered.truncated || "(empty)"),
      el(
        "span",
        { className: bytes >= NICKNAME_MAX_LEN ? "warn" : "dim" },
        `${bytes}/${NICKNAME_MAX_LEN}`,
      ),
    ),
    ...notes,
  ];
}

function syntaxReference() {
  const entry = (code: string, text: string) =>
    el(
      "div",
      { className: "kv-row" },
      el("span", { className: "mono" }, code),
      el("span", {}, text),
    );
  return el(
    "details",
    { className: "reference" },
    el("summary", {}, "Syntax reference"),
    entry("{expr}", "Evaluate an expression"),
    entry(
      "{..expr}",
      `Shrinks first when the name is over ${NICKNAME_MAX_LEN} characters`,
    ),
    entry("{expr:.1f}", "Format a number with fixed decimals"),
    entry(
      '{if x < 10: "a", < 20: "b", else: "c"}',
      "Conditional; later branches may drop the left side",
    ),
    entry(
      "Fields",
      "hypixel.displayname, hypixel.achievements.*, hypixel.stats.Bedwars.*, discord.name, blacklist.<tag>, coral.access (0 default to 5 owner)",
    ),
    entry("Operators", "+ - * / %, == != < <= > >=, and or not"),
  );
}
