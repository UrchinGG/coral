import { load } from "@/lib/api";
import { el, problem } from "@/lib/dom";
import type { Labels } from "@/lib/members";

export interface CursorPage<T> {
  items: T[];
  next_cursor: string | null;
  labels: Labels;
}

type Fetcher<T> = (cursor: string | null) => Promise<CursorPage<T>>;

export const getPages =
  <T>(path: string): Fetcher<T> =>
  (cursor) => {
    const join = path.includes("?") ? "&" : "?";
    return load<CursorPage<T>>(
      cursor ? `${path}${join}cursor=${encodeURIComponent(cursor)}` : path,
    );
  };

export function pager<T>(
  source: string | Fetcher<T>,
  render: (item: T, labels: Labels) => Node,
  empty: string,
) {
  const fetchPage = typeof source === "string" ? getPages<T>(source) : source;
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
    try {
      const page = await fetchPage(cursor);
      list.append(...page.items.map((item) => render(item, page.labels)));
      cursor = page.next_cursor;
      more.hidden = cursor === null;
      status.textContent = list.children.length ? "" : empty;
    } catch (error) {
      status.textContent = "";
      status.append(problem((error as Error).message, next));
    }
  };
  more.addEventListener("click", next);
  next();
  return el("div", { className: "list" }, list, status, more);
}
