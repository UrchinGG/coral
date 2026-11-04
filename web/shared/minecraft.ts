const palette: Record<string, string> = {
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

export type Run = {
  text: string;
  color: string;
  shadow: string;
  bold: boolean;
  strike: boolean;
};

const quarter = (hex: string) =>
  `#${hex
    .slice(1)
    .match(/../g)!
    .map((channel) =>
      (parseInt(channel, 16) >> 2).toString(16).padStart(2, "0"),
    )
    .join("")}`;

export function legacyRuns(text: string, base = palette.f): Run[] {
  let color = base;
  let bold = false;
  let strike = false;
  const runs: Run[] = [];
  const push = (chunk: string) =>
    chunk &&
    runs.push({ text: chunk, color, shadow: quarter(color), bold, strike });

  const [first, ...rest] = text.split("§");
  push(first);
  for (const part of rest) {
    const code = part.charAt(0).toLowerCase();
    if (code in palette || code === "r") {
      color = palette[code] ?? base;
      bold = strike = false;
    }
    if (code === "l") bold = true;
    if (code === "m") strike = true;
    push(part.slice(1));
  }
  return runs;
}

export const stripLegacy = (text: string) => text.replace(/§./g, "");
