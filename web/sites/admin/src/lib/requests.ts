import { load } from "@/lib/api";
import { el, pressable, problem } from "@/lib/dom";
import type { Labels } from "@/lib/members";
import { refLink, type Ref } from "@/lib/refs";
import { time, day } from "@/lib/time";

export interface Subject {
  player: string | null;
  name: string | null;
  raw: string;
}

export interface RequestRow {
  request_id: string;
  ts: string;
  method: string;
  route: string | null;
  path: string;
  status: number;
  cause: string;
  error: string | null;
  latency_ms: number;
  key_kind: string;
  key_prefix: string | null;
  requester: Ref;
  ip: Ref | null;
  country: string | null;
  user_agent: string | null;
  subjects: Subject[];
}

interface RequestFull {
  url: string;
  headers: Record<string, string> | null;
  body: string | null;
  detail_state: string;
  server_error: string | null;
}

const VIA_LABELS: Record<string, string> = {
  personal: "Key",
  developer: "Developer",
  internal: "Bot",
  session: "Session",
  minecraft: "Minecraft",
  unverified: "Unverified",
  invalid: "Bad key",
  legacy: "Legacy",
  none: "No key",
};

export const viaLabel = (keyKind: string) => VIA_LABELS[keyKind] ?? keyKind;

export const VIA_KINDS = Object.keys(VIA_LABELS);

const UPSTREAM_LIMIT = "upstream_limit";

const statusClass = (status: number, cause: string) =>
  status >= 500
    ? "s5xx"
    : status === 429
      ? cause === UPSTREAM_LIMIT
        ? "s429-upstream"
        : "s429"
      : status >= 400
        ? "s4xx"
        : "s2xx";

export function viaBadge(keyKind: string) {
  return el("span", {
    className: `via v-${keyKind}`,
    textContent: viaLabel(keyKind),
  });
}

export function methodBadge(method: string) {
  return el("span", {
    className: `method m-${method.toLowerCase()}`,
    textContent: method,
  });
}

function subjectCell(subjects: Subject[], labels: Labels) {
  const cell = el("span", { className: "player" });
  if (!subjects.length) return cell;
  const first = subjects[0];
  const name = first.player
    ? refLink({ kind: "player", uuid: first.player }, labels)
    : el("span", { textContent: first.name ?? first.raw });
  if (first.player && !labels.minecraft[first.player] && first.name)
    name.textContent = first.name;
  cell.append(name);
  if (subjects.length > 1) cell.append(` +${subjects.length - 1}`);
  return cell;
}

async function detail(row: RequestRow, box: HTMLElement) {
  box.textContent = "Loading…";
  try {
    const full = await load<RequestFull>(
      `/requests/${row.request_id}?t=${encodeURIComponent(row.ts)}`,
    );
    const fact = (label: string, value: string, className = "") =>
      el(
        "div",
        { className: "fact" },
        el("span", { className: "label", textContent: label }),
        el("span", { className, textContent: value }),
      );
    const facts = [
      fact("URL", full.url, "mono"),
      fact("Latency", `${row.latency_ms} ms`),
      fact(
        "Key",
        row.key_prefix ? `${row.key_prefix}… (${row.key_kind})` : row.key_kind,
      ),
      fact("Country", row.country ?? "Unknown"),
      fact("User agent", row.user_agent ?? "None", "mono"),
      fact("Cause", row.cause),
      ...(row.error ? [fact("Error", row.error, "bad")] : []),
      ...(full.server_error
        ? [fact("Server error", full.server_error, "bad")]
        : []),
    ];
    const headers = full.headers
      ? el("pre", {
          className: "code",
          textContent: Object.entries(full.headers)
            .map(([name, value]) => `${name}: ${value}`)
            .join("\n"),
        })
      : el("p", {
          className: "muted",
          textContent: `Headers ${full.detail_state.replace("_", " ")}.`,
        });
    const body = full.body
      ? el("pre", { className: "code", textContent: full.body })
      : "";
    box.replaceChildren(
      el("div", { className: "facts" }, ...facts),
      headers,
      body,
    );
  } catch (error) {
    box.replaceChildren(
      problem((error as Error).message, () => detail(row, box)),
    );
  }
}

export function requestRow(row: RequestRow, labels: Labels) {
  const box = el("div", { className: "row-detail", hidden: true });
  const line = el(
    "div",
    { className: "table-row request", title: day(row.ts) },
    el("span", { className: "dim", textContent: time(row.ts) }),
    el(
      "span",
      { className: "route" },
      methodBadge(row.method),
      row.route ?? row.path,
    ),
    refLink(row.requester, labels),
    subjectCell(row.subjects, labels),
    el(
      "span",
      {},
      el("span", {
        className: `status ${statusClass(row.status, row.cause)}`,
        textContent: String(row.status),
      }),
    ),
    ipCell(row.ip),
    el("span", {}, viaBadge(row.key_kind)),
  );
  line.setAttribute("aria-expanded", "false");
  pressable(line, () => {
    box.hidden = !box.hidden;
    line.setAttribute("aria-expanded", String(!box.hidden));
    if (!box.hidden && !box.childElementCount) detail(row, box);
  });
  return el("div", { className: "request-item" }, line, box);
}

function ipCell(ip: Ref | null) {
  const cell = el("span", { className: "mono" });
  if (ip?.kind === "ip")
    cell.append(refLink(ip, { discord: {}, minecraft: {} }));
  return cell;
}

export function requestHead() {
  return el(
    "div",
    { className: "table-row request head" },
    ...["Time", "Route", "Requester", "Player", "Status", "IP", "Via"].map(
      (label) => el("span", { textContent: label }),
    ),
  );
}

export { refLink };
