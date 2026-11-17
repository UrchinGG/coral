import { el, problem } from "@/lib/dom";

export function tabBar(
  root: HTMLElement,
  tabs: [string, string][],
  current: string,
  pick: (key: string) => void,
) {
  root.replaceChildren(
    ...tabs.map(([key, label]) => {
      const tab = el("button", {
        type: "button",
        role: "tab",
        textContent: label,
      });
      tab.setAttribute("aria-selected", String(key === current));
      tab.addEventListener("click", () => pick(key));
      return tab;
    }),
  );
}

export async function fillTab(
  container: HTMLElement,
  content: () => Promise<Node> | Node,
) {
  container.replaceChildren(
    el("p", { className: "muted", textContent: "Loading…" }),
  );
  try {
    container.replaceChildren(await content());
  } catch (error) {
    container.replaceChildren(
      problem((error as Error).message, () => fillTab(container, content)),
    );
  }
}

export function setQuery(name: string, value: string | null) {
  const params = new URLSearchParams(location.search);
  if (value === null) params.delete(name);
  else params.set(name, value);
  history.replaceState(
    null,
    "",
    `${location.pathname}${params.size ? `?${params}` : ""}`,
  );
}
