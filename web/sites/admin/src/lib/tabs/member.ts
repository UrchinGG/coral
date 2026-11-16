import { load, send } from "@/lib/api";
import { el, panel, row, widePanel } from "@/lib/dom";
import {
  TIME_RANGES,
  filterBar,
  rangeStart,
  type FilterState,
  type Pick,
} from "@/lib/filterbar";
import { FORMATS, titleCase } from "@/lib/format";
import type { Labels } from "@/lib/members";
import { pager, type CursorPage } from "@/lib/pager";
import { refLink, refName, type Ref } from "@/lib/refs";
import type { Budget } from "@/lib/tabs/keys";
import { tagPill } from "@/lib/tags";
import { ago, day, moment } from "@/lib/time";

interface TimelineEvent {
  key: string;
  at: string;
  kind: string;
  actor: Ref | null;
  target: Ref | null;
  target_text: string | null;
  player: Ref | null;
  action: string | null;
  detail: string | null;
  data: {
    tag_type?: string | null;
    expires_at?: string | null;
    ended_by?: string | null;
  } | null;
}

interface AddedTag {
  event: TimelineEvent;
  tagType: string;
  state: string;
}

const DEFAULT_RANGE = "30d";
const TAG_PAGE = 200;
const MAX_TAG_PAGES = 5;
const TAG_SET = "tag_set";
const TAG_STATES = ["Active", "Expired", "Removed", "Replaced"];
const TAG_COLUMNS = "minmax(120px,1fr) 130px minmax(180px,2fr) 100px 80px";

const KIND_GROUPS: Record<string, string> = {
  tag_authored: "Tags",
  tag_reviewed: "Tags",
  review_submitted: "Reviews",
  vote: "Reviews",
  strike_received: "Strikes",
  strike_revoked: "Strikes",
  strike_issued: "Strikes",
  key_created: "Keys",
  key_retired: "Keys",
  linked: "Links",
  unlinked: "Links",
  admin_action: "Staff",
  player_event: "Players",
};

export function timelineTab(id: string) {
  const state: FilterState = { q: "", picks: {}, range: DEFAULT_RANGE };
  const table = el("div", { className: "table" });
  const head = el(
    "div",
    { className: "table-row timeline head" },
    ...["When", "Kind", "Event", "By"].map((label) =>
      el("span", { textContent: label }),
    ),
  );
  let loaded = "";
  const visible = (node: HTMLElement) => {
    node.hidden = !matchesText(node, state.q);
    return node;
  };
  const show = () => {
    const key = `${state.picks.kind?.value}|${state.range}`;
    if (key === loaded) {
      table.querySelectorAll<HTMLElement>(".pages > *").forEach(visible);
      return;
    }
    loaded = key;
    table.replaceChildren(
      head,
      pager(
        `/accounts/${id}/timeline?${timelineQuery(state)}`,
        (event: TimelineEvent, labels) => visible(timelineRow(event, labels)),
        "Nothing happened on this account in this range.",
      ),
    );
  };
  const groups = [...new Set(Object.values(KIND_GROUPS))];
  const bar = filterBar({
    placeholder: "Search events, players and staff",
    state,
    ranges: TIME_RANGES,
    fields: [{ key: "kind", label: "Kind", values: () => groups.map(same) }],
    onChange: show,
  });
  show();
  return el("div", { className: "list" }, bar.element, table);
}

export function tagsTab(id: string) {
  const state: FilterState = { q: "", picks: {}, range: DEFAULT_RANGE };
  const body = el("div", { className: "stack" });
  let loadedRange: string | null = null;
  let tags: AddedTag[] = [];
  let labels: Labels = { discord: {}, minecraft: {} };
  const render = () => body.replaceChildren(...tagsView(tags, labels, state));
  const reload = async () => {
    loadedRange = state.range;
    body.replaceChildren(el("p", { className: "muted" }, "Loading…"));
    try {
      [tags, labels] = await addedTags(id, state.range);
      render();
    } catch (error) {
      body.replaceChildren(
        el("p", { className: "bad" }, (error as Error).message),
      );
    }
  };
  const bar = filterBar({
    placeholder: "Search players, tags and reasons",
    state,
    ranges: TIME_RANGES,
    fields: [
      {
        key: "tag",
        label: "Tag",
        values: () =>
          [...new Set(tags.map((tag) => tag.tagType))].map((type) => ({
            value: type,
            label: titleCase(type),
          })),
      },
      { key: "state", label: "State", values: () => TAG_STATES.map(same) },
    ],
    onChange: () => (state.range === loadedRange ? render() : reload()),
  });
  reload();
  return el("div", { className: "list" }, bar.element, body);
}

function timelineQuery(state: FilterState) {
  const query = new URLSearchParams();
  const group = state.picks.kind?.value;
  const kinds = Object.keys(KIND_GROUPS).filter(
    (kind) => KIND_GROUPS[kind] === group,
  );
  if (kinds.length) query.set("kinds", kinds.join(","));
  const from = rangeStart(state.range);
  if (from) query.set("from", from);
  return query;
}

async function addedTags(
  id: string,
  range: string | null,
): Promise<[AddedTag[], Labels]> {
  const query = new URLSearchParams({
    kinds: "tag_authored",
    limit: String(TAG_PAGE),
  });
  const from = rangeStart(range);
  if (from) query.set("from", from);
  const events: TimelineEvent[] = [];
  const labels: Labels = { discord: {}, minecraft: {} };
  let cursor: string | null = null;
  for (let page = 0; page < MAX_TAG_PAGES; page += 1) {
    const cursorQuery: string = cursor
      ? `&cursor=${encodeURIComponent(cursor)}`
      : "";
    const loaded: CursorPage<TimelineEvent> = await load(
      `/accounts/${id}/timeline?${query}${cursorQuery}`,
    );
    events.push(...loaded.items);
    Object.assign(labels.discord, loaded.labels.discord);
    Object.assign(labels.minecraft, loaded.labels.minecraft);
    cursor = loaded.next_cursor;
    if (!cursor) break;
  }
  const tags = events
    .filter((event) => event.action === TAG_SET && event.data?.tag_type)
    .map((event) => ({
      event,
      tagType: event.data!.tag_type!,
      state: tagState(event),
    }));
  return [tags, labels];
}

function tagState(event: TimelineEvent) {
  if (event.data?.ended_by === "tag_clear") return "Removed";
  if (event.data?.ended_by === TAG_SET) return "Replaced";
  const expires = event.data?.expires_at;
  return expires && Date.parse(expires) <= Date.now() ? "Expired" : "Active";
}

function tagsView(tags: AddedTag[], labels: Labels, state: FilterState) {
  const { tag: tagPick, state: statePick } = state.picks;
  const shown = tags.filter(
    (tag) =>
      (!tagPick || tagPick.value === tag.tagType) &&
      (!statePick || statePick.value === tag.state),
  );
  const rows = shown
    .map((tag) => [
      refLink(tag.event.player, labels),
      tagPill(tag.tagType, titleCase(tag.tagType)),
      tag.event.detail ?? "",
      day(tag.event.at),
      el("span", { className: tag.state === "Active" ? "" : "dim" }, tag.state),
    ])
    .map((cells) => gridRow(TAG_COLUMNS, "table-row", cells))
    .filter((line) => matchesText(line, state.q));
  const counts = TAG_STATES.map(
    (state) =>
      [state, tags.filter((tag) => tag.state === state).length] as const,
  );
  return [
    el(
      "div",
      { className: "stats" },
      ...counts.map(([state, count]) =>
        el(
          "div",
          { className: "stat" },
          el("span", { className: "label" }, state),
          el("b", {}, FORMATS.number(count)),
        ),
      ),
    ),
    el(
      "div",
      { className: "table" },
      gridRow(TAG_COLUMNS, "table-row head", [
        "Player",
        "Tag",
        "Reason",
        "Added",
        "State",
      ]),
      ...rows,
    ),
    ...(rows.length
      ? []
      : [
          el(
            "p",
            { className: "muted" },
            "No tags added by this member match.",
          ),
        ]),
  ];
}

const same = (value: string): Pick => ({ value, label: value });

function matchesText(node: Element, text: string) {
  return (node.textContent ?? "").toLowerCase().includes(text.toLowerCase());
}

function timelineRow(event: TimelineEvent, labels: Labels) {
  const what = [titleCase(event.action ?? event.kind), event.target_text ?? ""]
    .filter(Boolean)
    .join(": ");
  return el(
    "div",
    { className: "table-row timeline" },
    el("span", { className: "dim", textContent: moment(event.at) }),
    el("span", {
      textContent: KIND_GROUPS[event.kind] ?? titleCase(event.kind),
    }),
    el(
      "span",
      {},
      what,
      event.player ? " · " : "",
      event.player ? refLink(event.player, labels) : "",
      event.detail
        ? el("span", { className: "dim", textContent: ` · ${event.detail}` })
        : "",
    ),
    el("span", {}, event.actor ? refLink(event.actor, labels) : ""),
  );
}

interface StarfishAccount {
  user: {
    license_status: string;
    github_username: string | null;
    github_linked_at: string | null;
    created_at: string;
    updated_at: string;
  };
  hwids: {
    id: number;
    hwid_hash: string;
    is_active: boolean;
    registered_at: string;
    machine_guid_hash: string | null;
    smbios_uuid_hash: string | null;
    disk_serial_hash: string | null;
    cpu_id_hash: string | null;
    baseboard_serial_hash: string | null;
  }[];
  sessions: {
    id: number;
    hwid_id: number;
    issued_at: string;
    expires_at: string;
    last_heartbeat_at: string;
    active: boolean;
  }[];
  refresh_tokens: {
    hwid_id: number;
    created_at: string;
    last_used_at: string;
  }[];
  installed_plugins: {
    plugin: Ref;
    installed_version: string;
    latest_version: string;
    disabled: boolean;
  }[];
  owned_plugins: {
    plugin: Ref;
    official: boolean;
    unlisted: boolean;
    disabled: boolean;
    latest_version: string | null;
  }[];
}

const COMPONENTS: [string, keyof StarfishAccount["hwids"][number]][] = [
  ["Machine GUID", "machine_guid_hash"],
  ["SMBIOS UUID", "smbios_uuid_hash"],
  ["Disk serial", "disk_serial_hash"],
  ["CPU ID", "cpu_id_hash"],
  ["Baseboard serial", "baseboard_serial_hash"],
];

export async function starfishTab(id: string, refresh: () => void) {
  const account = await load<StarfishAccount | null>(
    `/accounts/${id}/starfish`,
  );
  if (!account)
    return el("p", { className: "muted", textContent: "No Starfish account." });
  const labels: Labels = { discord: {}, minecraft: {} };
  const license = account.user.license_status;
  const licenseButton = el("button", {
    type: "button",
    className: license === "active" ? "button small danger" : "button small",
    textContent: license === "active" ? "Suspend" : "Activate",
  });
  licenseButton.addEventListener("click", async () => {
    const status = license === "active" ? "suspended" : "active";
    if (
      confirm(`${licenseButton.textContent} this license?`) &&
      (await send(`/accounts/${id}/starfish/license`, "POST", { status }))
    )
      refresh();
  });
  const signOut = el("button", {
    type: "button",
    className: "button small",
    textContent: "Sign out everywhere",
  });
  signOut.addEventListener("click", async () => {
    if (
      confirm("End every Starfish session for this member?") &&
      (await send(`/accounts/${id}/starfish/sessions/revoke`, "POST", {}))
    )
      refresh();
  });
  const machine = (hwid: StarfishAccount["hwids"][number]) =>
    el(
      "details",
      { className: "machine" },
      el(
        "summary",
        {},
        el("span", {
          className: "mono",
          textContent: hwid.hwid_hash.slice(0, 16),
        }),
        el("span", {
          className: hwid.is_active ? "ok" : "dim",
          textContent: hwid.is_active ? "Active" : "Inactive",
        }),
        el("span", {
          className: "dim",
          textContent: `Registered ${day(hwid.registered_at)}`,
        }),
      ),
      ...COMPONENTS.map(([label, key]) =>
        row(
          label,
          el("span", {
            className: "mono",
            textContent: String(hwid[key] ?? "Not reported"),
          }),
        ),
      ),
    );
  return el(
    "div",
    { className: "panels" },
    panel(
      "License",
      row("Status", titleCase(license), licenseButton),
      row("Created", day(account.user.created_at)),
      row("Changed", day(account.user.updated_at)),
      row("GitHub", account.user.github_username ?? "Not linked"),
    ),
    panel(
      "Sessions",
      ...(account.sessions.length
        ? account.sessions.map((session) =>
            row(
              session.active ? "Active" : "Ended",
              `Heartbeat ${ago(session.last_heartbeat_at)}`,
              el("span", {
                className: "dim",
                textContent: ` · expires ${day(session.expires_at)}`,
              }),
            ),
          )
        : [row("None", "No sessions")]),
      el("div", { className: "kv-row" }, signOut),
    ),
    widePanel(
      "Machines · " + account.hwids.length,
      ...account.hwids.map(machine),
    ),
    panel(
      "Installed Plugins",
      ...(account.installed_plugins.length
        ? account.installed_plugins.map((plugin) =>
            row(
              refName(plugin.plugin, labels),
              plugin.installed_version === plugin.latest_version
                ? plugin.installed_version
                : `${plugin.installed_version}, latest ${plugin.latest_version}`,
            ),
          )
        : [row("None", "No plugins installed")]),
    ),
    panel(
      "Published Plugins",
      ...(account.owned_plugins.length
        ? account.owned_plugins.map((plugin) =>
            row(
              refName(plugin.plugin, labels),
              [
                plugin.official && "Official",
                plugin.unlisted && "Unlisted",
                plugin.disabled && "Disabled",
              ]
                .filter(Boolean)
                .join(", ") || "Listed",
            ),
          )
        : [row("None", "No plugins published")]),
    ),
  );
}

interface Network {
  ips: {
    ip: Ref;
    requests: number;
    first_seen: string;
    last_seen: string;
    shared_with: Ref[];
    shared_count: number;
  }[];
  agents: {
    user_agent: string;
    requests: number;
    first_seen: string;
    last_seen: string;
  }[];
  countries: { country: string | null; requests: number }[];
  linked_accounts: { account: Ref; ips: Ref[] }[];
  labels: Labels;
}

interface KeyRecord {
  kind: string;
  key: string;
  created_at: string | null;
  retired_at: string | null;
}

function gridRow(
  template: string,
  className: string,
  cells: (string | Node)[],
) {
  const line = el(
    "div",
    { className },
    ...cells.map((cell) => el("span", {}, cell)),
  );
  line.style.gridTemplateColumns = template;
  return line;
}

const table = (
  columns: string[],
  rows: (string | Node)[][],
  template: string,
) =>
  el(
    "div",
    { className: "table" },
    gridRow(template, "table-row head", columns),
    ...rows.map((cells) => gridRow(template, "table-row", cells)),
  );

export async function networkTab(id: string) {
  const [network, keys, limits] = await Promise.all([
    load<Network>(`/accounts/${id}/network`),
    load<KeyRecord[]>(`/accounts/${id}/keys`),
    load<Budget[]>(`/accounts/${id}/limits`),
  ]);
  const labels = network.labels;
  return el(
    "div",
    { className: "stack" },
    el(
      "div",
      { className: "panels" },
      panel(
        "Limits, live",
        ...limits.map((budget) =>
          row(
            titleCase(budget.limiter),
            budget.limit === null
              ? FORMATS.number(budget.used)
              : `${FORMATS.number(budget.used)} of ${FORMATS.number(budget.limit)} per ${budget.window_s / 60} min`,
          ),
        ),
      ),
      panel(
        "Key History",
        ...keys.map((key) =>
          row(
            `${titleCase(key.kind)}${key.retired_at ? `, retired ${day(key.retired_at)}` : ", current"}`,
            el("span", {
              className: "mono",
              textContent: `${key.key.slice(0, 8)}…`,
            }),
          ),
        ),
      ),
    ),
    el("h2", { className: "group-title", textContent: "IP Addresses" }),
    table(
      ["IP", "Requests", "First", "Last", "Shared With"],
      network.ips.map((use) => [
        refLink(use.ip, labels),
        FORMATS.number(use.requests),
        day(use.first_seen),
        ago(use.last_seen),
        use.shared_count
          ? el(
              "span",
              {},
              ...use.shared_with.flatMap((ref, index) => [
                index ? ", " : "",
                refLink(ref, labels),
              ]),
            )
          : "Only this account",
      ]),
      "minmax(120px,1fr) 90px 110px 110px minmax(140px,1.4fr)",
    ),
    el("h2", { className: "group-title", textContent: "User Agents" }),
    table(
      ["User Agent", "Requests", "First", "Last"],
      network.agents.map((agent) => [
        el("span", { className: "mono", textContent: agent.user_agent }),
        FORMATS.number(agent.requests),
        day(agent.first_seen),
        ago(agent.last_seen),
      ]),
      "minmax(200px,2fr) 90px 110px 110px",
    ),
    el("h2", { className: "group-title", textContent: "Countries" }),
    table(
      ["Country", "Requests"],
      network.countries.map((country) => [
        country.country ?? "Unknown",
        FORMATS.number(country.requests),
      ]),
      "minmax(120px,1fr) 120px",
    ),
  );
}

export function rawTab(member: { row: unknown; config: unknown } | null) {
  if (!member)
    return el("p", { className: "muted", textContent: "No member row." });
  return el(
    "div",
    { className: "panels" },
    widePanel(
      "Member Row",
      el("pre", {
        className: "code",
        textContent: JSON.stringify(member.row, null, 2),
      }),
    ),
    widePanel(
      "Config",
      el("pre", {
        className: "code",
        textContent: JSON.stringify(member.config, null, 2),
      }),
    ),
  );
}
