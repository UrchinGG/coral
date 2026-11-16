import { load } from "@/lib/api";
import { legend, stackedBars } from "@/lib/chart";
import { el } from "@/lib/dom";
import { filterChip } from "@/lib/filterbar";
import { FORMATS, titleCase } from "@/lib/format";
import type { Labels } from "@/lib/members";
import { pager } from "@/lib/pager";
import { refText, type Ref } from "@/lib/refs";
import { requestHead, requestRow, type RequestRow } from "@/lib/requests";
import { figures, type Figure } from "@/lib/snapshots";
import { fillTab } from "@/lib/tabbar";
import type { PlayerCore } from "@/lib/tabs/player";
import { ago, day, moment, time } from "@/lib/time";

type Window = "24h" | "7d" | "14d";

interface LookupSeries {
  clamped_from: string | null;
  series: {
    t: string;
    lookups: number;
    rate_limited: number;
    errors: number;
  }[];
}

interface Requester {
  key: string;
  requester: Ref;
  key_kind: string;
  requests: number;
  rate_limited: number;
  errors: number;
  first: string;
  last: string;
  forms: Record<string, number>;
}

interface Requesters {
  groups: Requester[];
  forms: Record<string, number>;
  uncaptured_batches: number | null;
  labels: Labels;
}

interface StreamFilter {
  forms: Set<string>;
  requester: Requester | null;
}

type PlayerRequestRow = RequestRow & { matched_form: string };

const WINDOWS: [Window, string][] = [
  ["24h", "24 hours"],
  ["7d", "7 days"],
  ["14d", "14 days"],
];
const REQUESTER_WINDOW = "14 d";
const COLORS = { answered: "#3a9a5b", limited: "#e5c74a", errors: "#e66767" };
const FORM_LABELS: Record<string, string> = {
  name: "Name",
  former: "Former Name",
  undashed: "UUID",
  dashed: "Dashed UUID",
  path: "Path",
  body: "Batch Body",
  display: "Display Name",
};
const FORM_ORDER = Object.keys(FORM_LABELS);
const STANDARD_FORMS = FORM_ORDER.filter((form) => form !== "display");
const STATUSES: [string, string][] = [
  ["", "Every status"],
  ["2xx", "2xx"],
  ["429", "429"],
  ["4xx", "Other 4xx"],
  ["5xx", "5xx"],
];
const RATE_LIMITED = 429;
const MATCHED_COLUMN = 3;
const STATUS_CLASS_SIZE = 100;

const formLabel = (form: string) => FORM_LABELS[form] ?? titleCase(form);
const sum = (values: number[]) => values.reduce((a, b) => a + b, 0);

export function lookupPreview(core: PlayerCore, open: () => void) {
  const holder = el("div", { className: "panel wide lookup-preview" });
  fillTab(holder, async () => {
    const [series, requesters] = await Promise.all([
      load<LookupSeries>(`/players/${core.uuid}/lookups?window=24h`),
      load<Requesters>(`/players/${core.uuid}/requesters`),
    ]);
    const openButton = el("button", {
      type: "button",
      className: "button small",
      textContent: "Open Lookups ›",
    });
    openButton.addEventListener("click", open);
    return lookupChart(
      series,
      "Lookups, last 24 hours",
      openButton,
      previewFigures(series, requesters),
    );
  });
  return holder;
}

export function lookupsTab(core: PlayerCore) {
  const chart = el("div", { className: "panel wide" });
  const detail = el("div", { className: "stack" });
  const filter: StreamFilter = {
    forms: new Set(STANDARD_FORMS),
    requester: null,
  };
  const showChart = (window: Window) =>
    fillTab(chart, async () => {
      const data = await load<LookupSeries>(
        `/players/${core.uuid}/lookups?window=${window}`,
      );
      const label = WINDOWS.find(([key]) => key === window)![1];
      return lookupChart(
        data,
        `Lookups of ${core.name ?? core.dashed}, last ${label}`,
        windowPicker(window, showChart),
        tabFigures(data, core, label),
      );
    });
  const showDetail = () =>
    fillTab(detail, () => requestDetail(core.uuid, filter, showDetail));
  showChart("24h");
  showDetail();
  return el("div", { className: "stack" }, chart, detail);
}

function windowPicker(current: Window, pick: (window: Window) => void) {
  const picker = el("div", { className: "seg" });
  picker.setAttribute("role", "group");
  picker.setAttribute("aria-label", "Time range");
  for (const [key, label] of WINDOWS) {
    const button = el("button", {
      type: "button",
      className: key === current ? "on" : "",
      textContent: label,
    });
    button.addEventListener("click", () => pick(key));
    picker.append(button);
  }
  return picker;
}

function lookupChart(
  data: LookupSeries,
  title: string,
  control: Node,
  stats: Figure[],
) {
  const limited = data.series.map((point) => point.rate_limited);
  const errors = data.series.map((point) => point.errors);
  const series = [
    {
      label: "Answered",
      color: COLORS.answered,
      values: data.series.map((point, index) =>
        Math.max(0, point.lookups - limited[index] - errors[index]),
      ),
    },
    { label: "Rate limited", color: COLORS.limited, values: limited },
    { label: "Errors", color: COLORS.errors, values: errors },
  ];
  return el(
    "div",
    { className: "stack" },
    el(
      "div",
      { className: "panel-head" },
      el("h2", { className: "panel-title", textContent: title }),
      control,
    ),
    legend(series),
    stackedBars(
      series,
      data.series.map((point) => moment(point.t)),
    ),
    ...(data.clamped_from
      ? [
          el("p", {
            className: "muted",
            textContent: `Lookups before ${moment(data.clamped_from)} weren't recorded by player.`,
          }),
        ]
      : []),
    figures(stats),
  );
}

function tabFigures(
  data: LookupSeries,
  core: PlayerCore,
  label: string,
): Figure[] {
  return [
    [
      "Lookups",
      FORMATS.number(sum(data.series.map((point) => point.lookups))),
      `last ${label}`,
    ],
    [
      "Rate Limited",
      FORMATS.number(sum(data.series.map((point) => point.rate_limited))),
      "answered 429",
    ],
    [
      "Errors",
      FORMATS.number(sum(data.series.map((point) => point.errors))),
      "answered 5xx",
    ],
    [
      "Requesters",
      FORMATS.number(core.requests.requesters_14d),
      `last ${REQUESTER_WINDOW}`,
    ],
  ];
}

function previewFigures(
  series: LookupSeries,
  requesters: Requesters,
): Figure[] {
  const groups = requesters.groups;
  const top = groups.reduce<Requester | null>(
    (best, group) => (!best || group.requests > best.requests ? group : best),
    null,
  );
  const latest = groups.reduce<Requester | null>(
    (best, group) => (!best || group.last > best.last ? group : best),
    null,
  );
  const by = (group: Requester | null, text: string) =>
    group ? refText(group.requester, requesters.labels) : text;
  return [
    [
      "Lookups",
      FORMATS.number(sum(series.series.map((point) => point.lookups))),
      "last 24 h",
    ],
    ["Requesters", FORMATS.number(groups.length), `last ${REQUESTER_WINDOW}`],
    [
      "Top Requester",
      by(top, "None"),
      top ? `${FORMATS.number(top.requests)} lookups` : "",
    ],
    [
      "Last Lookup",
      latest ? time(latest.last) : "None",
      latest ? el("span", {}, `${ago(latest.last)} by `, by(latest, "")) : "",
    ],
  ];
}

async function requestDetail(
  uuid: string,
  filter: StreamFilter,
  redraw: () => void,
) {
  const data = await load<Requesters>(
    `/players/${uuid}/requesters?${formsParam(filter.forms)}`,
  );
  const pick = (group: Requester) => {
    filter.requester = filter.requester?.key === group.key ? null : group;
    redraw();
  };
  return el(
    "div",
    { className: "stack" },
    el("h2", { className: "group-title" }, `Requesters · ${REQUESTER_WINDOW}`),
    requestersTable(data, filter.requester, pick),
    el("h2", { className: "group-title" }, "Requests · Matched By"),
    formChips(data, filter, redraw),
    ...(data.uncaptured_batches ? [batchWarning(data.uncaptured_batches)] : []),
    streamTable(uuid, filter, redraw, data.labels),
  );
}

function formsParam(forms: Set<string>, extra: [string, string][] = []) {
  const standard =
    forms.size === STANDARD_FORMS.length &&
    STANDARD_FORMS.every((form) => forms.has(form));
  const chosen = FORM_ORDER.filter((form) => forms.has(form));
  return new URLSearchParams([
    ...(standard ? [] : [["forms", chosen.join(",")]]),
    ...extra,
  ]);
}

function requestersTable(
  data: Requesters,
  picked: Requester | null,
  pick: (group: Requester) => void,
) {
  const head = el(
    "div",
    { className: "table-row requesters head" },
    ...[
      "Requester",
      "Via",
      "Lookups",
      "429",
      "Errors",
      "First",
      "Last",
      "Matched By",
    ].map((label) => el("span", { textContent: label })),
  );
  const rows = data.groups.map((group) => {
    const line = el(
      "button",
      {
        type: "button",
        className: "table-row requesters",
        title: "Show only this requester's lookups",
      },
      refText(group.requester, data.labels),
      el("span", { textContent: titleCase(group.key_kind) }),
      el("span", {
        className: "num",
        textContent: FORMATS.number(group.requests),
      }),
      el("span", {
        className: "num",
        textContent: FORMATS.number(group.rate_limited),
      }),
      el("span", {
        className: "num",
        textContent: FORMATS.number(group.errors),
      }),
      el("span", { textContent: day(group.first) }),
      el("span", { textContent: ago(group.last) }),
      el("span", {
        className: "dim",
        textContent: Object.keys(group.forms).map(formLabel).join(", "),
      }),
    );
    line.setAttribute("aria-pressed", String(picked?.key === group.key));
    line.addEventListener("click", () => pick(group));
    return line;
  });
  return el(
    "div",
    { className: "table" },
    head,
    ...(rows.length
      ? rows
      : [
          el("p", {
            className: "muted pad",
            textContent: "Nobody looked this player up.",
          }),
        ]),
  );
}

function formChips(data: Requesters, filter: StreamFilter, redraw: () => void) {
  const forms = FORM_ORDER.filter((form) => form in data.forms);
  const chips = forms.map((form) => {
    const on = filter.forms.has(form);
    const chip = el(
      "button",
      { type: "button", className: "chip" },
      formLabel(form),
      el("span", {
        className: "dim",
        textContent: ` ${FORMATS.short(data.forms[form])}`,
      }),
    );
    chip.setAttribute("aria-pressed", String(on));
    chip.disabled = on && filter.forms.size === 1;
    chip.addEventListener("click", () => {
      if (on) filter.forms.delete(form);
      else filter.forms.add(form);
      redraw();
    });
    return chip;
  });
  const group = el("div", { className: "chips" }, ...chips);
  group.setAttribute("role", "group");
  group.setAttribute("aria-label", "Matched by");
  return group;
}

function batchWarning(uncaptured: number) {
  return el("p", {
    className: "warn-line",
    textContent: `${FORMATS.number(uncaptured)} batch requests in this window were never captured, so they could also include this player.`,
  });
}

function streamTable(
  uuid: string,
  filter: StreamFilter,
  redraw: () => void,
  labels: Labels,
) {
  const search = el("input", {
    className: "field grow",
    type: "search",
    placeholder: "Search requesters, routes, statuses",
    ariaLabel: "Search requests",
  });
  const status = selectOf("Status", STATUSES);
  const via = selectOf("Via", [["", "Every key kind"]]);
  const vias = new Set<string>();
  const shown = (item: HTMLElement) =>
    item
      .textContent!.toLowerCase()
      .includes(search.value.trim().toLowerCase()) &&
    (!status.value || item.dataset.status === status.value) &&
    (!via.value || item.dataset.via === via.value);
  const render = (row: PlayerRequestRow, rowLabels: Labels) => {
    if (!vias.has(row.key_kind)) {
      vias.add(row.key_kind);
      via.append(
        el("option", {
          value: row.key_kind,
          textContent: titleCase(row.key_kind),
        }),
      );
    }
    const item = streamRow(row, rowLabels);
    item.hidden = !shown(item);
    return item;
  };
  const list = pager<PlayerRequestRow>(
    `/players/${uuid}/requests?${formsParam(filter.forms, filter.requester ? [["requester", filter.requester.key]] : [])}`,
    render,
    `Nobody looked this player up in ${REQUESTER_WINDOW}.`,
  );
  const refilter = () =>
    list
      .querySelectorAll<HTMLElement>(".request-item")
      .forEach((item) => (item.hidden = !shown(item)));
  search.addEventListener("input", refilter);
  status.addEventListener("change", refilter);
  via.addEventListener("change", refilter);
  return el(
    "div",
    { className: "stack" },
    el(
      "div",
      { className: "toolbar" },
      search,
      status,
      via,
      ...(filter.requester ? [requesterChip(filter, redraw, labels)] : []),
    ),
    el("div", { className: "table" }, streamHead(), list),
  );
}

function streamRow(row: PlayerRequestRow, labels: Labels) {
  const item = requestRow(row, labels);
  const cells = item.querySelector(".table-row")!.children;
  cells[MATCHED_COLUMN].replaceWith(
    el("span", { textContent: formLabel(row.matched_form) }),
  );
  item.dataset.status =
    row.status === RATE_LIMITED
      ? String(RATE_LIMITED)
      : `${Math.floor(row.status / STATUS_CLASS_SIZE)}xx`;
  item.dataset.via = row.key_kind;
  return item;
}

function streamHead() {
  const head = requestHead();
  head.children[MATCHED_COLUMN].textContent = "Matched By";
  return head;
}

function requesterChip(
  filter: StreamFilter,
  redraw: () => void,
  labels: Labels,
) {
  return filterChip(
    "Show every requester",
    () => {
      filter.requester = null;
      redraw();
    },
    "Requester: ",
    refText(filter.requester!.requester, labels),
  );
}

function selectOf(label: string, options: [string, string][]) {
  return el(
    "select",
    { className: "field", ariaLabel: label },
    ...options.map(([value, text]) =>
      el("option", { value, textContent: text }),
    ),
  );
}
