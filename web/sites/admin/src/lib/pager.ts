import { load } from "@/lib/api";
import { el } from "@/lib/dom";
import type { Labels } from "@/lib/members";

interface CursorPage<T> {
  items: T[];
  next_cursor: string | null;
  labels: Labels;
}

export function pager<T>(
  path: string,
  render: (item: T, labels: Labels) => Node,
  empty: string,
) {
  const list = el("div", { className: "pages" });
  const status = el("p", { className: "muted", textContent: "Loading…" });
  const more = el("button", {
    type: "button",
    className: "button",
    textContent: "Load more",
    hidden: true,
  });
  let cursor: string | null = null;
  const next = async () => {
    const join = path.includes("?") ? "&" : "?";
    try {
      const page = await load<CursorPage<T>>(
        cursor ? `${path}${join}cursor=${encodeURIComponent(cursor)}` : path,
      );
      list.append(...page.items.map((item) => render(item, page.labels)));
      cursor = page.next_cursor;
      more.hidden = cursor === null;
      status.textContent = list.children.length ? "" : empty;
    } catch (error) {
      status.textContent = (error as Error).message;
    }
  };
  more.addEventListener("click", next);
  next();
  return el("div", { className: "list" }, list, status, more);
}
