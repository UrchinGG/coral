import { legend, stackedArea } from "@/lib/chart";
import { el, panel, widePanel } from "@/lib/dom";
import { FORMATS } from "@/lib/format";
import { ago } from "@/lib/time";

type Tone =
  "neutral" | "subtle" | "ok" | "info" | "warning" | "limited" | "danger";

interface Field {
  name: string;
  label: string;
  required: boolean;
  kind: "text" | "secret" | "select";
  placeholder?: string | null;
  value?: string | null;
  options?: { value: string; label: string }[];
}

interface Action {
  action: string;
  label: string;
  danger: boolean;
  input: Record<string, unknown>;
  fields: Field[];
  confirm: { title: string; text: string; phrase: string | null } | null;
}

type Cell =
  | { kind: "text"; text: string; tone: Tone }
  | { kind: "mono"; text: string }
  | { kind: "number"; value: number; suffix: string | null; tone: Tone }
  | { kind: "pill"; text: string; tone: Tone }
  | { kind: "meter"; used: number; limit: number }
  | { kind: "time"; at: string }
  | { kind: "secret"; value: string };

type Section =
  | {
      type: "notice";
      level: "info" | "warning" | "danger";
      text: string;
      actions: Action[];
    }
  | {
      type: "stats";
      items: { label: string; value: string; detail: string | null }[];
    }
  | {
      type: "table";
      id: string;
      title: string;
      columns: { key: string; label: string; numeric: boolean }[];
      rows: { key: string; cells: Record<string, Cell>; actions: Action[] }[];
    }
  | { type: "form"; id: string; title: string; fields: Field[]; submit: Action }
  | {
      type: "series";
      id: string;
      title: string;
      unit: string | null;
      lines: {
        label: string;
        tone: Tone;
        points: { t: string; value: number }[];
      }[];
    };

export interface ExtensionView {
  title: string;
  subtitle: string | null;
  sections: Section[];
}

export type Run = (action: string, input: Record<string, unknown>) => void;

const TONE_CLASS: Record<Tone, string> = {
  neutral: "",
  subtle: "dim",
  ok: "ok",
  info: "",
  warning: "warn",
  limited: "warn",
  danger: "bad",
};
const TONE_COLOR: Record<Tone, string> = {
  neutral: "#9a9da4",
  subtle: "#6a6d74",
  ok: "#3a9a5b",
  info: "#5c9dff",
  warning: "#ffaa00",
  limited: "#e8a23a",
  danger: "#e66767",
};
const NOTICE_CLASS = { info: "", warning: "warn", danger: "bad" };
const WARN_SHARE = 0.9;
const ACTION_WIDTH_PX = 92;

function input(field: Field) {
  if (field.kind === "select") {
    const select = el("select", { className: "field", name: field.name });
    select.append(
      ...(field.options ?? []).map((option) =>
        el("option", {
          value: option.value,
          textContent: option.label,
          selected: option.value === field.value,
        }),
      ),
    );
    return select;
  }
  return el("input", {
    className: "field",
    name: field.name,
    type: field.kind === "secret" ? "password" : "text",
    placeholder: field.placeholder ?? "",
    value: field.value ?? "",
    required: field.required,
  });
}

function trigger(
  action: Action,
  run: Run,
  values: Record<string, unknown> = {},
) {
  if (action.confirm) {
    const typed = action.confirm.phrase
      ? prompt(
          `${action.confirm.text}\n\nType ${action.confirm.phrase} to confirm.`,
        )
      : confirm(`${action.confirm.title}\n\n${action.confirm.text}`)
        ? ""
        : null;
    if (
      typed === null ||
      (action.confirm.phrase && typed !== action.confirm.phrase)
    )
      return;
  }
  run(action.action, { ...action.input, ...values });
}

function actionButton(action: Action, run: Run) {
  const button = el("button", {
    type: "button",
    className: action.danger ? "button small danger" : "button small",
    textContent: action.label,
  });
  if (!action.fields.length) {
    button.addEventListener("click", () => trigger(action, run));
    return button;
  }
  const form = el(
    "form",
    { className: "inline-fields", hidden: true },
    ...action.fields.map(input),
    el("button", { className: "button small", textContent: "Save" }),
  );
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    trigger(action, run, Object.fromEntries(new FormData(form)));
  });
  button.addEventListener("click", () => (form.hidden = !form.hidden));
  return el("span", { className: "inline" }, button, form);
}

function cell(value: Cell | undefined): Node {
  if (!value) return el("span");
  switch (value.kind) {
    case "text":
      return el("span", {
        className: TONE_CLASS[value.tone],
        textContent: value.text,
      });
    case "mono":
      return el("span", { className: "mono", textContent: value.text });
    case "number":
      return el("span", {
        className: `num ${TONE_CLASS[value.tone]}`,
        textContent: `${FORMATS.number(value.value)}${value.suffix ?? ""}`,
      });
    case "pill":
      return el("span", {
        className: `pill ${TONE_CLASS[value.tone]}`,
        textContent: value.text,
      });
    case "time":
      return el("span", { className: "dim", textContent: ago(value.at) });
    case "meter": {
      const share = value.limit ? value.used / value.limit : 0;
      const fill = el("span");
      fill.style.width = `${Math.min(share, 1) * 100}%`;
      return el(
        "span",
        { className: "meter-cell" },
        el(
          "span",
          { className: share > WARN_SHARE ? "meter warn" : "meter ok" },
          fill,
        ),
        el("span", {
          className: "dim mono",
          textContent: `${value.used} / ${value.limit}`,
        }),
      );
    }
    case "secret": {
      const text = el("span", { className: "mono", textContent: "••••••••" });
      const show = el("button", {
        type: "button",
        className: "button small",
        textContent: "Show",
      });
      show.addEventListener("click", () => {
        const hidden = show.textContent === "Show";
        text.textContent = hidden ? value.value : "••••••••";
        show.textContent = hidden ? "Hide" : "Show";
      });
      return el("span", { className: "inline" }, text, show);
    }
  }
}

function section(part: Section, run: Run): Node {
  switch (part.type) {
    case "notice":
      return el(
        "div",
        { className: `notice-row ${NOTICE_CLASS[part.level]}` },
        el("span", { textContent: part.text }),
        el(
          "span",
          { className: "actions" },
          ...part.actions.map((action) => actionButton(action, run)),
        ),
      );
    case "stats":
      return el(
        "div",
        { className: "stats" },
        ...part.items.map((item) =>
          el(
            "div",
            { className: "stat" },
            el("span", { className: "label", textContent: item.label }),
            el("b", { textContent: item.value }),
            item.detail
              ? el("span", { className: "muted", textContent: item.detail })
              : "",
          ),
        ),
      );
    case "table": {
      const mostActions = Math.max(
        0,
        ...part.rows.map((row) => row.actions.length),
      );
      const hasActions = mostActions > 0;
      const template = `${part.columns.map((column) => (column.numeric ? "minmax(70px, 0.6fr)" : "minmax(100px, 1fr)")).join(" ")}${hasActions ? ` ${mostActions * ACTION_WIDTH_PX}px` : ""}`;
      const line = (cells: Node[], head = false) => {
        const row = el(
          "div",
          { className: head ? "table-row head" : "table-row" },
          ...cells,
        );
        row.style.gridTemplateColumns = template;
        return row;
      };
      return el(
        "section",
        { className: "list" },
        el("h2", { className: "group-title", textContent: part.title }),
        el(
          "div",
          { className: "table" },
          line(
            part.columns
              .map((column) =>
                el("span", {
                  className: column.numeric ? "num" : "",
                  textContent: column.label,
                }),
              )
              .concat(hasActions ? [el("span")] : []),
            true,
          ),
          ...part.rows.map((row) =>
            line([
              ...part.columns.map((column) => cell(row.cells[column.key])),
              ...(hasActions
                ? [
                    el(
                      "span",
                      { className: "actions" },
                      ...row.actions.map((action) => actionButton(action, run)),
                    ),
                  ]
                : []),
            ]),
          ),
        ),
      );
    }
    case "form": {
      const form = el(
        "form",
        { className: "inline-form" },
        ...part.fields.map((field) =>
          el(
            "label",
            { className: "ffield" },
            el("span", { className: "label", textContent: field.label }),
            input(field),
          ),
        ),
        el("button", { className: "button", textContent: part.submit.label }),
      );
      form.addEventListener("submit", (event) => {
        event.preventDefault();
        trigger(part.submit, run, Object.fromEntries(new FormData(form)));
      });
      return panel(part.title, form);
    }
    case "series":
      return widePanel(
        part.title,
        legend(
          part.lines.map((line) => ({
            label: line.label,
            color: TONE_COLOR[line.tone],
          })),
        ),
        stackedArea(
          part.lines.map((line) => ({
            label: line.label,
            color: TONE_COLOR[line.tone],
            values: line.points.map((point) => point.value),
          })),
          null,
        ),
      );
  }
}

export function renderView(view: ExtensionView, run: Run) {
  return el(
    "div",
    { className: "stack" },
    ...(view.subtitle
      ? [el("p", { className: "muted", textContent: view.subtitle })]
      : []),
    ...view.sections.map((part) => section(part, run)),
  );
}
