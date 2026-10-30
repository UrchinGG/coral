const gray = "#aaaaaa";

const rankColors: Record<string, [string, string?]> = {
  VIP: ["#55ff55"],
  "VIP+": ["#55ff55", "#ffaa00"],
  MVP: ["#55ffff"],
  "MVP+": ["#55ffff", "#ff5555"],
  "MVP++": ["#ffaa00", "#ffaa00"],
};

const nameColor = (rank: string | null) =>
  (rank && rankColors[rank]?.[0]) || gray;

function rankParts(rank: string | null) {
  if (!rank) return [];
  const [base, plus = base] = rankColors[rank] ?? [gray];
  const name = rank.replace(/\+/g, "");
  return [
    { text: `[${name}`, color: base },
    { text: rank.slice(name.length), color: plus },
    { text: "]", color: base },
  ].filter((part) => part.text);
}

export function showName(
  element: HTMLElement,
  name: string,
  rank: string | null,
) {
  const parts = rankParts(rank).map(({ text, color }) => {
    const part = document.createElement("span");
    part.className = "rank";
    part.style.color = color;
    part.textContent = text;
    return part;
  });
  element.style.color = nameColor(rank);
  element.replaceChildren(...parts, name);
}
