export const ACCESS_LEVELS: Record<number, string> = {
  0: "Default",
  2: "Helper",
  3: "Moderator",
  4: "Admin",
  5: "Owner",
};

export const accessLabel = (level: number) =>
  ACCESS_LEVELS[
    Object.keys(ACCESS_LEVELS)
      .map(Number)
      .filter((rank) => rank <= level)
      .at(-1) ?? 0
  ];

export interface Labels {
  discord: Record<string, string>;
  minecraft: Record<string, string>;
}

export const memberUrl = (discordId: string) =>
  `/urchin/members?id=${discordId}`;
