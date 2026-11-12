export interface Team {
  name: string;
  color: string;
  order: number;
  known: boolean;
}

const TEAMS = [
  { name: "Red", color: "#ff5555", dyes: ["#ff0000", "#ff5555", "#aa0000"] },
  { name: "Blue", color: "#5555ff", dyes: ["#0000ff", "#5555ff", "#0000aa"] },
  {
    name: "Green",
    color: "#55ff55",
    dyes: ["#48cc18", "#55ff55", "#00aa00", "#008000"],
  },
  { name: "Yellow", color: "#ffff55", dyes: ["#ffff00", "#ffff55"] },
  { name: "Aqua", color: "#55ffff", dyes: ["#00ffff", "#55ffff", "#00aaaa"] },
  { name: "White", color: "#ffffff", dyes: ["#ffffff"] },
  {
    name: "Pink",
    color: "#ff55ff",
    dyes: ["#ef83a4", "#ff55ff", "#aa00aa", "#800080"],
  },
  { name: "Gray", color: "#555555", dyes: ["#808080", "#555555", "#aaaaaa"] },
].map(({ name, color, dyes }, order) => ({
  team: { name, color, order, known: true },
  dyes,
}));

export const teamDyed = (color: string): Team =>
  TEAMS.find(({ dyes }) => dyes.includes(color))?.team ?? {
    name: "Other",
    color,
    order: TEAMS.length,
    known: false,
  };

export const teamNamed = (name: string): Team =>
  TEAMS.find(({ team }) => team.name === name)?.team ?? {
    name,
    color: "#ffffff",
    order: TEAMS.length,
    known: false,
  };
