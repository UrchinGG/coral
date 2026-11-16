import { call } from "@/lib/api";
import { el } from "@/lib/dom";
import { FORMATS } from "@/lib/format";

export interface Role {
  id: string;
  name: string;
  color: string | null;
  position: number;
  managed: boolean;
  assignable: boolean;
  bot_assignable: boolean | null;
}

export interface Channel {
  id: string;
  name: string;
  category: string | null;
  kind: "text" | "forum";
}

export interface Directory {
  as_of: string;
  guild: { id: string; name: string; icon_url: string | null };
  roles: Role[];
  channels: Channel[];
}

export interface Population {
  as_of: string;
  member_count: number;
  linked_members: number;
  role_members: Record<string, number>;
}

export interface Rule {
  id: number;
  role_id: string;
  condition: string;
}

export interface ServerConfig {
  guild_id: string;
  enabled: boolean;
  config: {
    link_role_id: string | null;
    unlinked_role_id: string | null;
    link_channel_id: string | null;
    nickname_template: string | null;
  };
  rules: Rule[];
  source_ids: string[];
  preview_context: Record<string, unknown> | null;
}

export interface Job {
  id: number;
  kind: string;
  label: string;
  state: "queued" | "running" | "done" | "cancelled" | "failed";
  processed: number;
  total: number;
  error: string | null;
  started_at: string;
  finished_at: string | null;
}

export interface Server {
  base: string;
  config: ServerConfig;
  directory: Directory | null;
  population: Population | null;
  refresh: () => void;
  report: (text: string, bad?: boolean) => void;
}

export function membersPhrase(server: Server) {
  return server.population
    ? `${FORMATS.number(server.population.member_count)} members`
    : "every member";
}

export async function apply(
  server: Server,
  path: string,
  method: string,
  body: unknown,
  done: string,
) {
  try {
    const answer = await call<{ job: Job | null } | null>(
      `${server.base}${path}`,
      method,
      body,
    );
    server.report(
      answer?.job ? `${done}; started "${answer.job.label}".` : `${done}.`,
    );
    server.refresh();
    return true;
  } catch (error) {
    server.report((error as Error).message, true);
    return false;
  }
}

export function roleSelect(
  server: Server,
  value: string | null,
  taken: Set<string> = new Set(),
) {
  const roles = server.directory?.roles ?? [];
  return select(
    value,
    "No role",
    roles.map((role) => {
      const hint = taken.has(role.id)
        ? "has a rule"
        : role.managed
          ? "managed"
          : role.assignable
            ? ""
            : "not manageable";
      return el("option", {
        value: role.id,
        textContent: `@${role.name}${hint ? ` (${hint})` : ""}`,
        disabled: Boolean(hint) && role.id !== value,
      });
    }),
  );
}

export function channelSelect(server: Server, value: string | null) {
  const channels = (server.directory?.channels ?? []).filter(
    (channel) => channel.kind === "text",
  );
  return select(
    value,
    "No channel",
    channels.map((channel) =>
      el("option", {
        value: channel.id,
        textContent: `#${channel.name}${channel.category ? ` · ${channel.category}` : ""}`,
      }),
    ),
  );
}

export function roleChip(server: Server, id: string) {
  const role = server.directory?.roles.find((entry) => entry.id === id);
  const dot = el("span", { className: "role-dot" });
  dot.style.background = role?.color ?? "var(--color-text-faint)";
  return el(
    "span",
    { className: "role-chip" },
    dot,
    role ? `@${role.name}` : `@${id}`,
  );
}

export function smallButton(
  label: string,
  run: () => unknown,
  className = "button small",
) {
  const button = el("button", {
    type: "button",
    className,
    textContent: label,
  });
  button.addEventListener("click", run);
  return button;
}

function select(
  value: string | null,
  empty: string,
  options: HTMLOptionElement[],
) {
  const element = el(
    "select",
    { className: "field" },
    el("option", { value: "", textContent: empty }),
    ...options,
  );
  element.value = value ?? "";
  if (value && element.value !== value)
    element.append(el("option", { value, textContent: value, selected: true }));
  return element;
}
