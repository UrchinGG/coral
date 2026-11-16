export const DISCORD_URL = "https://discord.gg/jgKEVUJj3H";

export const pagePath = ({ pathname }: URL) =>
  pathname.replace(/(\/index)?\.html$/, "") || "/";
