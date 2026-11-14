export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Partial<HTMLElementTagNameMap[K]> & { className?: string } = {},
  ...children: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const element = Object.assign(document.createElement(tag), props);
  element.append(...children);
  return element;
}

export function row(label: string, ...value: (Node | string)[]) {
  return el(
    "div",
    { className: "kv-row" },
    el("span", { className: "dim" }, label),
    el("span", {}, ...value),
  );
}

export function panel(title: string, ...rows: Node[]) {
  return el(
    "section",
    { className: "panel" },
    el("h2", { className: "panel-title" }, title),
    ...rows,
  );
}

export function widePanel(title: string, ...rows: Node[]) {
  const section = panel(title, ...rows);
  section.classList.add("wide");
  return section;
}
