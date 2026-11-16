import { FORMATS, type Format } from "@/lib/format";

const at = (data: unknown, path: string) =>
  path
    .split(".")
    .reduce<unknown>(
      (value, key) => (value as Record<string, unknown> | undefined)?.[key],
      data,
    );

export function fill(root: ParentNode, data: unknown) {
  for (const element of root.querySelectorAll<HTMLElement>("[data-value]")) {
    const value = at(data, element.dataset.value!);
    if (value === undefined || value === null) continue;
    const format = FORMATS[(element.dataset.format ?? "number") as Format] as (
      value: never,
    ) => string;
    element.textContent = format(value as never);
  }
}

export function failed(root: Element, message: string) {
  root.classList.add("failed");
  root.setAttribute("title", message);
}
