export function showCode(root: Element, code: string) {
  root.querySelector(".code code")!.textContent = code;
  root.querySelector<HTMLAnchorElement>("a.primary")!.href =
    `https://www.microsoft.com/link?otc=${encodeURIComponent(code)}`;
  root.querySelector<HTMLElement>(".copy")!.dataset.copy = code;
}
