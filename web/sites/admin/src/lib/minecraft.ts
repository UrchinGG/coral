import { el } from "@/lib/dom";

const SECTION_SIGN = "§";
const DEFAULT_COLOR = "#aaaaaa";

const COLORS: Record<string, string> = {
  "0": "#000000",
  "1": "#0000aa",
  "2": "#00aa00",
  "3": "#00aaaa",
  "4": "#aa0000",
  "5": "#aa00aa",
  "6": "#ffaa00",
  "7": "#aaaaaa",
  "8": "#555555",
  "9": "#5555ff",
  a: "#55ff55",
  b: "#55ffff",
  c: "#ff5555",
  d: "#ff55ff",
  e: "#ffff55",
  f: "#ffffff",
};

export const head = (uuid: string, size: number) =>
  `https://mc-heads.net/avatar/${uuid}/${size}`;

export function displayName(prefix: string | null, name: string) {
  const parts = colored(`${prefix ?? ""}${name}`);
  return el("span", { className: "mc-name" }, ...parts);
}

function colored(text: string) {
  let color = DEFAULT_COLOR;
  return text.split(SECTION_SIGN).flatMap((chunk, index) => {
    const code = index === 0 ? undefined : chunk[0]?.toLowerCase();
    const body = index === 0 ? chunk : chunk.slice(1);
    if (code === "r") color = DEFAULT_COLOR;
    else if (code && COLORS[code]) color = COLORS[code];
    if (!body) return [];
    const part = el("span", { textContent: body });
    part.style.color = color;
    return [part];
  });
}
