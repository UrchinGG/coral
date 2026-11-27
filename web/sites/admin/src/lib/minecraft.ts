import { legacyRuns, palette } from "@shared/minecraft";

import { el } from "@/lib/dom";

const DEFAULT_COLOR = palette["7"];
const OUTLINED_COLORS = new Set([palette["0"], palette["1"]]);

export const UNKNOWN_PLAYER = "Unknown player";

export const head = (uuid: string, size: number) =>
  `https://mc-heads.net/avatar/${uuid}/${size}`;

export function displayName(prefix: string | null, name: string) {
  const parts = legacyRuns(`${prefix ?? ""}${name}`, DEFAULT_COLOR).map(
    ({ text, color }) => {
      const part = el("span", { textContent: text });
      part.style.color = color;
      part.classList.toggle("outlined", OUTLINED_COLORS.has(color));
      return part;
    },
  );
  return el("span", { className: "mc-name" }, ...parts);
}
