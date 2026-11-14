import { load } from "@/lib/api";
import { el, panel, row, widePanel } from "@/lib/dom";
import { FORMATS, titleCase } from "@/lib/format";
import type { Labels } from "@/lib/members";
import { pager } from "@/lib/pager";
import { refLink, type Ref } from "@/lib/refs";
import { requestHead, requestRow, type RequestRow } from "@/lib/requests";
import { ago, day, moment } from "@/lib/time";

export interface Tag {
  id: number;
  tag_type: string;
  display_name: string;
  color: string | null;
  reason: string | null;
  author: Ref | null;
  at: string;
  expires_at: string | null;
  hide_username: boolean;
  reviewed_by: Ref[];
}

export interface Hold {
  by: Ref | null;
  at: string;
  reason: string | null;
}

export interface PlayerCore {
  uuid: string;
  dashed: string;
  name: string | null;
  names: {
    name: string;
    first_seen: string | null;
    last_seen: string | null;
    current: boolean;
  }[];
  owners: {
    account: Ref;
    relation: string;
    linked_at: string | null;
    unlinked_at: string | null;
    primary_shared_with: number;
  }[];
  blacklist: { active: Tag[]; locked: Hold | null };
  minecraft_block: Hold | null;
  snapshots: { count: number; first: string | null; last: string | null };
  requests: { last_24h: number; last_14d: number; requesters_14d: number };
  labels: Labels;
}

interface PlayerTags {
  history: (Tag & {
    ended: { how: string; by: Ref | null; at: string } | null;
  })[];
  locks: {
    locked: boolean;
    by: Ref | null;
    at: string;
    reason: string | null;
  }[];
  labels: Labels;
}

export function overviewTab(core: PlayerCore, removeTag: (tag: Tag) => void) {
  const labels = core.labels;
  const tags = core.blacklist.active.map((tag) => {
    const remove = el("button", {
      type: "button",
      className: "button small danger",
      textContent: "Remove",
    });
    remove.addEventListener("click", () => removeTag(tag));
    return row(
      tag.display_name,
      tag.reason ?? "",
      el(
        "span",
        { className: "dim" },
        " · ",
        tag.author ? refLink(tag.author, labels) : "unknown",
        ` · ${day(tag.at)}${tag.expires_at ? `, expires ${day(tag.expires_at)}` : ""}`,
      ),
      remove,
    );
  });
  const owners = core.owners.map((owner) =>
    row(
      owner.unlinked_at
        ? `Former ${owner.relation}`
        : titleCase(owner.relation),
      refLink(owner.account, labels),
      el("span", {
        className: "dim",
        textContent: owner.primary_shared_with
          ? ` · shares a primary with ${owner.primary_shared_with}`
          : "",
      }),
    ),
  );
  const names = core.names.map((name) =>
    row(
      name.current ? "Current" : "Former",
      name.name,
      el("span", {
        className: "dim",
        textContent: name.first_seen
          ? ` · ${day(name.first_seen)}${name.last_seen && !name.current ? ` to ${day(name.last_seen)}` : ""}`
          : "",
      }),
    ),
  );
  return el(
    "div",
    { className: "panels" },
    widePanel(
      "Tags",
      ...(tags.length ? tags : [row("None", "No active tags")]),
    ),
    panel(
      "Linked To",
      ...(owners.length
        ? owners
        : [row("Nobody", "Not linked to an Urchin account")]),
    ),
    panel("Names", ...names),
    panel(
      "Data",
      row("Snapshots", FORMATS.number(core.snapshots.count)),
      row(
        "First seen",
        core.snapshots.first ? day(core.snapshots.first) : "Never",
      ),
      row(
        "Last snapshot",
        core.snapshots.last ? ago(core.snapshots.last) : "Never",
      ),
      row("Lookups, 24 h", FORMATS.number(core.requests.last_24h)),
      row("Lookups, 14 d", FORMATS.number(core.requests.last_14d)),
      row("Requesters, 14 d", FORMATS.number(core.requests.requesters_14d)),
    ),
  );
}

export async function tagsTab(uuid: string) {
  const tags = await load<PlayerTags>(`/players/${uuid}/tags`);
  const labels = tags.labels;
  const history = tags.history.map((tag) =>
    row(
      tag.display_name,
      tag.reason ?? "",
      el(
        "span",
        { className: "dim" },
        " · ",
        tag.author ? refLink(tag.author, labels) : "unknown",
        ` · ${day(tag.at)}`,
        tag.ended ? ` · ${tag.ended.how} ${day(tag.ended.at)}` : " · active",
      ),
    ),
  );
  const locks = tags.locks.map((lock) =>
    row(
      lock.locked ? "Locked" : "Unlocked",
      lock.reason ?? "",
      el(
        "span",
        { className: "dim" },
        " · ",
        lock.by ? refLink(lock.by, labels) : "",
        ` · ${moment(lock.at)}`,
      ),
    ),
  );
  return el(
    "div",
    { className: "panels" },
    widePanel(
      "Tag History",
      ...(history.length ? history : [row("None", "Never tagged")]),
    ),
    widePanel(
      "Lock History",
      ...(locks.length ? locks : [row("None", "Never locked")]),
    ),
  );
}

interface SnapshotPage {
  days: { day: string; count: number }[];
  rows: {
    ts: string;
    source: string | null;
    baseline: boolean;
    username: string | null;
  }[];
  next_cursor: string | null;
}

export async function snapshotsTab(uuid: string) {
  const page = await load<SnapshotPage>(`/players/${uuid}/snapshots`);
  const viewer = el("pre", { className: "code", hidden: true });
  const open = async (ts: string) => {
    viewer.hidden = false;
    viewer.textContent = "Loading…";
    const snapshot = await load<{ data: unknown }>(
      `/players/${uuid}/snapshots/at?ts=${encodeURIComponent(ts)}`,
    );
    viewer.textContent = JSON.stringify(snapshot.data, null, 2);
  };
  const peak = Math.max(1, ...page.days.map((entry) => entry.count));
  const bars = el(
    "div",
    { className: "bars tall" },
    ...page.days.map((entry) => {
      const bar = el("span", {
        title: `${entry.day}: ${entry.count} snapshots`,
      });
      bar.style.height = `${(entry.count / peak) * 100}%`;
      return bar;
    }),
  );
  const rows = page.rows.map((snapshot) => {
    const view = el("button", {
      type: "button",
      className: "button small",
      textContent: "View",
    });
    view.addEventListener("click", () => open(snapshot.ts));
    return row(
      moment(snapshot.ts),
      snapshot.username ?? "",
      el("span", {
        className: "dim",
        textContent: ` · ${snapshot.baseline ? "full" : "change"}${snapshot.source ? ` · ${snapshot.source}` : ""}`,
      }),
      view,
    );
  });
  return el(
    "div",
    { className: "stack" },
    panel("Snapshots per day", bars),
    el(
      "div",
      { className: "panels" },
      panel(
        "Recent Snapshots",
        ...(rows.length ? rows : [row("None", "No snapshots")]),
      ),
      viewer,
    ),
  );
}

export function requestsTab(uuid: string) {
  return el(
    "div",
    { className: "table" },
    requestHead(),
    pager<RequestRow>(
      `/players/${uuid}/requests?window=14d`,
      requestRow,
      "Nobody looked this player up in 14 days.",
    ),
  );
}

interface Requesters {
  groups: {
    key: string;
    requester: Ref;
    key_kind: string;
    requests: number;
    rate_limited: number;
    errors: number;
    first: string;
    last: string;
  }[];
  labels: Labels;
}

export async function requestersTab(uuid: string) {
  const data = await load<Requesters>(`/players/${uuid}/requesters?window=14d`);
  const line = (cells: (string | Node)[], head = false) =>
    el(
      "div",
      { className: `table-row requesters${head ? " head" : ""}` },
      ...cells.map((cell) => el("span", {}, cell)),
    );
  return el(
    "div",
    { className: "table" },
    line(
      ["Requester", "Via", "Lookups", "429", "Errors", "First", "Last"],
      true,
    ),
    ...data.groups.map((group) =>
      line([
        refLink(group.requester, data.labels),
        titleCase(group.key_kind),
        FORMATS.number(group.requests),
        FORMATS.number(group.rate_limited),
        FORMATS.number(group.errors),
        day(group.first),
        ago(group.last),
      ]),
    ),
  );
}
