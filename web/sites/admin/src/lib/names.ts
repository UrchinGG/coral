import { API } from "@/lib/paths";

type Kind = "discord" | "minecraft";

interface Resolved {
  discord: Record<string, string>;
  minecraft: Record<string, string>;
  pending: { discord: string[]; minecraft: string[] };
}

const ATTRIBUTE: Record<Kind, string> = {
  discord: "data-pending-discord",
  minecraft: "data-pending-minecraft",
};
const BATCH = 200;
const RETRY_DELAYS_MS = [2000, 4000, 8000, 15000];
const GATHER_DELAY_MS = 50;

const known: Record<Kind, Map<string, string>> = {
  discord: new Map(),
  minecraft: new Map(),
};
const attempts: Record<Kind, Map<string, number>> = {
  discord: new Map(),
  minecraft: new Map(),
};
let scheduled: ReturnType<typeof setTimeout> | undefined;

export function markPending(node: HTMLElement, kind: Kind, id: string) {
  const name = known[kind].get(id);
  if (name) {
    node.textContent = name;
    return node;
  }
  node.setAttribute(ATTRIBUTE[kind], id);
  schedule();
  return node;
}

export function watchNames(root: HTMLElement) {
  new MutationObserver(schedule).observe(root, {
    childList: true,
    subtree: true,
  });
  schedule();
}

function schedule() {
  clearTimeout(scheduled);
  scheduled = setTimeout(resolve, GATHER_DELAY_MS);
}

function waiting(kind: Kind) {
  const ids = new Set<string>();
  for (const node of document.querySelectorAll(`[${ATTRIBUTE[kind]}]`))
    ids.add(node.getAttribute(ATTRIBUTE[kind])!);
  const asked = attempts[kind];
  const due = [...ids]
    .filter((id) => (asked.get(id) ?? 0) <= RETRY_DELAYS_MS.length)
    .slice(0, BATCH);
  for (const id of due) asked.set(id, (asked.get(id) ?? 0) + 1);
  return due;
}

async function resolve() {
  const discord = waiting("discord");
  const minecraft = waiting("minecraft");
  if (!discord.length && !minecraft.length) return;
  const answer = await ask(discord, minecraft);
  if (!answer) return;
  apply("discord", answer.discord);
  apply("minecraft", answer.minecraft);
  const retry = retryDelay([
    ...answer.pending.discord.map((id) => attempts.discord.get(id) ?? 0),
    ...answer.pending.minecraft.map((id) => attempts.minecraft.get(id) ?? 0),
  ]);
  if (retry !== undefined) setTimeout(resolve, retry);
}

function retryDelay(tries: number[]) {
  if (!tries.length) return undefined;
  return RETRY_DELAYS_MS[Math.min(...tries) - 1];
}

async function ask(discord: string[], minecraft: string[]) {
  const response = await fetch(`${API}/labels`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ discord, minecraft }),
  }).catch(() => undefined);
  return response?.ok ? ((await response.json()) as Resolved) : undefined;
}

function apply(kind: Kind, names: Record<string, string>) {
  for (const [id, name] of Object.entries(names)) {
    known[kind].set(id, name);
    for (const node of document.querySelectorAll<HTMLElement>(
      `[${ATTRIBUTE[kind]}="${CSS.escape(id)}"]`,
    )) {
      node.textContent = name;
      node.removeAttribute(ATTRIBUTE[kind]);
    }
  }
}
