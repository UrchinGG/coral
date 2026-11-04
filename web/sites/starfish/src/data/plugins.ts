export type Plugin = {
  name: string;
  prefix: string | null;
  description: string;
};

export const featuredPlugins: Plugin[] = [
  {
    name: "Urchin",
    prefix: "§5BL",
    description: "Urchin blacklist integration & API utilities",
  },
  {
    name: "BedWars",
    prefix: "§cB§fW",
    description: "Quality of life features for BedWars",
  },
  {
    name: "Stats Overlay",
    prefix: null,
    description: "Reworked tablist that shows players' stats ingame",
  },
  {
    name: "Cheater Detector",
    prefix: "§cAC",
    description: "Scans players to detect patterns that can indicate cheating",
  },
  {
    name: "Nick Alerts",
    prefix: "§cDN",
    description: "Detects and resolves nicked players when possible",
  },
];
