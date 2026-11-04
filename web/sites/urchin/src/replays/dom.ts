export const $ = <E extends Element = HTMLElement>(selector: string) =>
  document.querySelector<E>(selector)!;

export const h = <K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Partial<HTMLElementTagNameMap[K]> = {},
  ...children: (Node | string)[]
) => {
  const element = Object.assign(document.createElement(tag), props);
  element.append(...children);
  return element;
};

export const clock = (ms: number) =>
  `${Math.floor(ms / 60000)}:${String(Math.floor(ms / 1000) % 60).padStart(2, "0")}`;

export const countUntil = (items: { t: number }[], time: number) => {
  let low = 0;
  let high = items.length;
  while (low < high) {
    const middle = (low + high) >> 1;
    if (items[middle].t <= time) low = middle + 1;
    else high = middle;
  }
  return low;
};

export const afterPaint = () =>
  new Promise((resolve) => requestAnimationFrame(() => setTimeout(resolve)));

export const safely = <T>(use: () => T) => {
  try {
    return use();
  } catch {
    return undefined;
  }
};
