export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Partial<HTMLElementTagNameMap[K]> & { className?: string } = {},
  ...children: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const element = Object.assign(document.createElement(tag), props);
  element.append(...children);
  return element;
}

export function row(label: Node | string, ...value: (Node | string)[]) {
  return el(
    "div",
    { className: "kv-row" },
    el("span", { className: "dim" }, label),
    el("span", {}, ...value),
  );
}

export function pressable(element: HTMLElement, press: () => void) {
  element.setAttribute("role", "button");
  element.tabIndex = 0;
  element.addEventListener("click", (event) => {
    if (!(event.target as Element).closest("a")) press();
  });
  element.addEventListener("keydown", (event) => {
    if (event.target !== element) return;
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    press();
  });
  return element;
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

/** problem is a load failure shown in place, with Retry when the load can be tried again. */
export function problem(message: string, retry?: () => void) {
  const box = el(
    "div",
    { className: "problem" },
    el("span", { textContent: message }),
  );
  box.setAttribute("role", "alert");
  if (retry) {
    const again = el("button", {
      type: "button",
      className: "button small",
      textContent: "Retry",
    });
    again.addEventListener("click", () => {
      box.remove();
      retry();
    });
    box.append(again);
  }
  return box;
}
