import { el } from "@/lib/dom";

export function tagPill(tagType: string, displayName: string) {
  return el("span", {
    className: `tag-pill tag-${tagType}`,
    textContent: displayName,
  });
}
