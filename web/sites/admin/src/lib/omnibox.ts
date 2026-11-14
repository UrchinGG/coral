import { el } from "@/lib/dom";
import { refName, refUrl } from "@/lib/refs";
import { search, searchMojang } from "@/lib/search";

const SEARCH_DELAY_MS = 250;
const MINECRAFT_NAME = /^[a-z0-9_]{3,16}$/i;
const KIND_LABELS: Record<string, string> = {
  account: "Member",
  player: "Player",
  guild: "Guild",
  plugin: "Plugin",
  ip: "IP",
  name: "Player",
};

export function omnibox(input: HTMLInputElement) {
  const menu = el("div", {
    className: "omnibox",
    hidden: true,
    role: "listbox",
  });
  input.after(menu);
  let first: string | null = null;
  let generation = 0;

  async function run(q: string) {
    const mine = ++generation;
    if (!q) return (menu.hidden = true);
    const response = await search(q).catch(() => null);
    if (mine !== generation || !response) return;
    let hits = response.hits.filter((hit) => refUrl(hit.ref));
    if (!hits.length && MINECRAFT_NAME.test(q))
      hits = (await searchMojang(q).catch(() => [])).filter((hit) =>
        refUrl(hit.ref),
      );
    if (mine !== generation) return;
    first = hits.length ? refUrl(hits[0].ref) : null;
    menu.replaceChildren(
      ...(hits.length
        ? hits.map((hit) =>
            el(
              "a",
              {
                href: refUrl(hit.ref)!,
                className: "omnibox-hit",
                role: "option",
              },
              el("span", {
                className: "dim",
                textContent: KIND_LABELS[hit.ref.kind] ?? hit.ref.kind,
              }),
              el("span", {
                className: "strong",
                textContent: refName(hit.ref, response.labels),
              }),
              el("span", { className: "muted", textContent: hit.reason }),
            ),
          )
        : [
            el("p", {
              className: "muted",
              textContent: `Nothing matches "${q}".`,
            }),
          ]),
    );
    menu.hidden = false;
  }

  let typing: ReturnType<typeof setTimeout>;
  input.addEventListener("input", () => {
    clearTimeout(typing);
    typing = setTimeout(() => run(input.value.trim()), SEARCH_DELAY_MS);
  });
  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && first) location.href = first;
    if (event.key === "Escape") menu.hidden = true;
  });
  document.addEventListener("click", (event) => {
    if (!menu.contains(event.target as Node) && event.target !== input)
      menu.hidden = true;
  });
  document.addEventListener("keydown", (event) => {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
      event.preventDefault();
      input.focus();
      input.select();
    }
  });
}
