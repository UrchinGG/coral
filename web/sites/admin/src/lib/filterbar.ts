import "./filterbar.css";
import { el } from "@/lib/dom";

export interface Pick {
  value: string;
  label: string;
}

export type Picks = Record<string, Pick>;

export interface FilterField {
  key: string;
  label: string;
  values?: () => Pick[] | Promise<Pick[]>;
  parse?: (raw: string) => Pick | null;
}

export interface FilterState {
  q: string;
  picks: Picks;
  range: string | null;
}

interface FilterBarOptions {
  placeholder: string;
  hint?: string;
  fields: FilterField[];
  ranges?: [string, string][];
  state: FilterState;
  onChange: () => void;
}

const TYPING_DELAY_MS = 300;
const FIELD_TERM = /^([a-z]+):(.+)$/i;
const DAY_MS = 86_400_000;
const RANGE_DAYS: Record<string, number> = { "24h": 1, "7d": 7, "30d": 30 };

export const TIME_RANGES: [string, string][] = [
  ["24h", "24 h"],
  ["7d", "7 d"],
  ["30d", "30 d"],
  ["all", "All"],
];

export function rangeStart(range: string | null) {
  const days = RANGE_DAYS[range ?? ""];
  return days ? new Date(Date.now() - days * DAY_MS).toISOString() : null;
}

export function filterBar(options: FilterBarOptions) {
  const { state, fields } = options;
  const input = el("input", {
    className: "field fbar-search",
    type: "search",
    placeholder: options.placeholder,
    value: state.q,
  });
  input.setAttribute("aria-label", options.placeholder);
  const chips = el("span", { className: "fbar-chips" });
  const menu = el("div", { className: "fbar-menu", hidden: true });
  const add = el("button", {
    type: "button",
    className: "button small",
    textContent: "+ Filter",
  });
  const adder = el("div", { className: "fbar-add" }, add, menu);
  const ranges = el("div", { className: "seg" });
  ranges.setAttribute("role", "group");
  ranges.setAttribute("aria-label", "Time range");
  ranges.hidden = !options.ranges;
  const clear = el("button", {
    type: "button",
    className: "button small ghost",
    textContent: "Clear",
  });

  const changed = () => {
    render();
    options.onChange();
  };

  function set(key: string, pick: Pick | null) {
    if (pick) state.picks[key] = pick;
    else delete state.picks[key];
    changed();
  }

  function openMenu(open: boolean) {
    menu.hidden = !open;
    add.setAttribute("aria-expanded", String(open));
    if (open) showFields();
  }

  function absorb(everything: boolean) {
    const terms = input.value.split(/\s+/);
    const complete = everything ? terms : terms.slice(0, -1);
    const rest = complete.filter((term) => term && !takeTerm(term));
    if (!everything) rest.push(terms.at(-1) ?? "");
    input.value = rest.join(" ");
    state.q = input.value.trim();
  }

  function takeTerm(term: string) {
    const match = FIELD_TERM.exec(term);
    const field = match && fields.find((f) => f.key === match[1].toLowerCase());
    if (!match || !field) return false;
    const pick = parseTerm(field, match[2]);
    if (pick) state.picks[field.key] = pick;
    return pick !== null;
  }

  function render() {
    chips.replaceChildren(
      ...fields
        .filter((field) => state.picks[field.key])
        .map((field) =>
          filterChip(
            `Remove the ${field.label} filter`,
            () => set(field.key, null),
            `${field.label}: ${state.picks[field.key].label}`,
          ),
        ),
    );
    ranges.replaceChildren(...(options.ranges ?? []).map(rangeButton));
    clear.hidden = !state.q && !Object.keys(state.picks).length;
  }

  function rangeButton([key, label]: [string, string]) {
    const button = el("button", {
      type: "button",
      textContent: label,
      className: key === state.range ? "on" : "",
    });
    button.setAttribute("aria-pressed", String(key === state.range));
    button.addEventListener("click", () => {
      state.range = key;
      changed();
    });
    return button;
  }

  function showFields() {
    const open = fields.filter((field) => !state.picks[field.key]);
    menu.replaceChildren(
      el("span", { className: "label", textContent: "Filter By" }),
      ...open.map((field) =>
        menuItem(field.label, "›", () =>
          field.values ? showValues(field) : typeField(field),
        ),
      ),
    );
  }

  async function showValues(field: FilterField) {
    menu.replaceChildren(
      el("span", { className: "label", textContent: field.label }),
      el("span", { className: "muted", textContent: "Loading…" }),
    );
    const values = await field.values!();
    menu.replaceChildren(
      el("span", { className: "label", textContent: field.label }),
      ...(values.length
        ? values.map((pick) =>
            menuItem(pick.label, "", () => {
              openMenu(false);
              set(field.key, pick);
            }),
          )
        : [el("span", { className: "muted", textContent: "Nothing to pick" })]),
    );
  }

  function typeField(field: FilterField) {
    openMenu(false);
    input.value = `${input.value.trim()} ${field.key}:`.trimStart();
    input.focus();
  }

  let typing: ReturnType<typeof setTimeout>;
  input.addEventListener("input", () => {
    clearTimeout(typing);
    typing = setTimeout(() => {
      absorb(false);
      changed();
    }, TYPING_DELAY_MS);
  });
  input.addEventListener("change", () => {
    clearTimeout(typing);
    absorb(true);
    changed();
  });
  add.addEventListener("click", () => openMenu(menu.hidden === true));
  clear.addEventListener("click", () => {
    input.value = "";
    state.q = "";
    state.picks = {};
    changed();
  });
  document.addEventListener("click", (event) => {
    if (!event.composedPath().includes(adder)) openMenu(false);
  });
  openMenu(false);
  render();

  const element = el(
    "div",
    { className: "fbar" },
    el("div", { className: "fbar-row" }, input, chips, adder, ranges, clear),
    ...(options.hint
      ? [el("div", { className: "fbar-hint", textContent: options.hint })]
      : []),
  );
  return { element, set, render };
}

export function filterChip(
  removeLabel: string,
  remove: () => void,
  ...content: (Node | string)[]
) {
  const button = el("button", {
    type: "button",
    className: "fbar-chip-x",
    textContent: "×",
  });
  button.setAttribute("aria-label", removeLabel);
  button.addEventListener("click", remove);
  return el("span", { className: "fbar-chip" }, ...content, button);
}

function parseTerm(field: FilterField, raw: string): Pick | null {
  if (field.parse) return field.parse(raw);
  const values = field.values?.();
  if (!Array.isArray(values)) return { value: raw, label: raw };
  const wanted = raw.toLowerCase();
  return (
    values.find(
      (pick) =>
        pick.value.toLowerCase() === wanted ||
        pick.label.toLowerCase() === wanted,
    ) ?? null
  );
}

function menuItem(text: string, hint: string, pick: () => void) {
  const button = el(
    "button",
    { type: "button" },
    el("span", { textContent: text }),
    el("span", { className: "dim", textContent: hint }),
  );
  button.addEventListener("click", pick);
  return button;
}
