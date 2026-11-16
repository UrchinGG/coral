import { load } from "@/lib/api";
import { el, panel, row, widePanel } from "@/lib/dom";
import { FORMATS, titleCase } from "@/lib/format";
import type { Labels } from "@/lib/members";
import { refLink, type Ref } from "@/lib/refs";
import {
  busiest,
  monthChart,
  monthLabel,
  snapshotList,
  type MonthCount,
  type SnapshotEntry,
  type SnapshotView,
} from "@/lib/snapshots";
import { lookupPreview } from "@/lib/tabs/player-lookups";
import { tagPill } from "@/lib/tags";
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
  snapshots: {
    count: number;
    first: string | null;
    last: string | null;
    months: MonthCount[];
  };
  guild: {
    guild: Ref;
    tag: string | null;
    rank: string | null;
    joined: string | null;
  } | null;
  requests: { last_24h: number; last_14d: number; requesters_14d: number };
  labels: Labels;
}

export interface OverviewActions {
  removeTag?: (tag: Tag) => void;
  open: (tab: string, month?: string) => void;
  lookups: boolean;
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

export function overviewTab(core: PlayerCore, actions: OverviewActions) {
  return el(
    "div",
    { className: "panels" },
    widePanel("Tags", ...activeTags(core, actions.removeTag)),
    panel("Linked To", ...owners(core)),
    panel("Names", ...names(core)),
    ...(actions.lookups
      ? [lookupPreview(core, () => actions.open("lookups"))]
      : []),
    snapshotPreview(core, actions.open),
  );
}

function activeTags(core: PlayerCore, removeTag?: (tag: Tag) => void) {
  const tags = core.blacklist.active.map((tag) => {
    const remove = el("button", {
      type: "button",
      className: "button small danger",
      textContent: "Remove",
      hidden: !removeTag,
    });
    remove.addEventListener("click", () => removeTag?.(tag));
    return row(
      tagPill(tag.tag_type, tag.display_name),
      tag.reason ?? "",
      el(
        "span",
        { className: "dim" },
        " · ",
        tag.author ? refLink(tag.author, core.labels) : "unknown",
        ` · ${day(tag.at)}${tag.expires_at ? `, expires ${day(tag.expires_at)}` : ""}`,
      ),
      remove,
    );
  });
  return tags.length ? tags : [row("None", "No active tags")];
}

function owners(core: PlayerCore) {
  const rows = core.owners.map((owner) =>
    row(
      owner.unlinked_at
        ? `Former ${owner.relation}`
        : titleCase(owner.relation),
      refLink(owner.account, core.labels),
      el("span", {
        className: "dim",
        textContent: owner.primary_shared_with
          ? ` · shares a primary with ${owner.primary_shared_with}`
          : "",
      }),
    ),
  );
  return rows.length
    ? rows
    : [row("Nobody", "Not linked to an Urchin account")];
}

function names(core: PlayerCore) {
  return core.names.map((name) =>
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
}

function snapshotPreview(core: PlayerCore, open: OverviewActions["open"]) {
  const top = busiest(core.snapshots.months);
  return monthChart({
    title: "Snapshots, all time",
    unit: "Snapshots per month",
    months: core.snapshots.months,
    pick: (month) => open("snapshots", month),
    action: ["Open Snapshots ›", () => open("snapshots")],
    figures: [
      ["Snapshots", FORMATS.number(core.snapshots.count), "all time"],
      [
        "First Seen",
        core.snapshots.first ? day(core.snapshots.first) : "Never",
        "first snapshot",
      ],
      [
        "Last Snapshot",
        core.snapshots.last ? moment(core.snapshots.last) : "Never",
        core.snapshots.last ? ago(core.snapshots.last) : "",
      ],
      [
        "Busiest Month",
        top ? monthLabel(top.month) : "None",
        top ? `${FORMATS.number(top.count)} snapshots` : "",
      ],
    ],
  });
}

export async function tagsTab(uuid: string) {
  const tags = await load<PlayerTags>(`/players/${uuid}/tags`);
  const labels = tags.labels;
  const history = tags.history.map((tag) =>
    row(
      tagPill(tag.tag_type, tag.display_name),
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
  rows: SnapshotEntry[];
  next_cursor: string | null;
}

export function snapshotsTab(
  core: PlayerCore,
  month: string | null,
  pick: (month: string | null) => void,
) {
  const base = `/players/${core.uuid}/snapshots`;
  const params = (cursor: string | null) =>
    new URLSearchParams([
      ...(month ? [["month", month]] : []),
      ...(cursor ? [["cursor", cursor]] : []),
    ]);
  return el(
    "div",
    { className: "stack" },
    monthChart({
      title: "Snapshots, all time",
      count: core.snapshots.first
        ? `${FORMATS.number(core.snapshots.count)} since ${day(core.snapshots.first)}`
        : "",
      unit: "Snapshots per month. Pick a month to list only its snapshots.",
      months: core.snapshots.months,
      picked: month,
      pick: (picked) => pick(picked === month ? null : picked),
      action: month ? ["Show all time", () => pick(null)] : undefined,
    }),
    snapshotList({
      title: month ? monthLabel(month) : "Every Snapshot",
      pages: (cursor) => load<SnapshotPage>(`${base}?${params(cursor)}`),
      view: (entry) =>
        load<SnapshotView>(`${base}/at?ts=${encodeURIComponent(entry.ts)}`),
    }),
  );
}
