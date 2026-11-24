export const DISCORD_URL = "https://discord.gg/jgKEVUJj3H";
export const STARFISH_TERMS_URL = "https://urchin.gg/starfish/terms";
export const PAYNOW_BUYER_AGREEMENT_URL =
  "https://paynow.gg/legal/buyer-agreement";

export const pagePath = ({ pathname }: URL) =>
  pathname.replace(/(\/index)?\.html$/, "") || "/";
