import type { AccountRef, Labels } from "./refs";
import type { AgentUse } from "./accounts";

export type IpAccount = { account: AccountRef; requests: number; first_seen: string; last_seen: string; imported: boolean };

export type IpCore = {
  ip: string;
  country: string | null;
  proxied: boolean;
  accounts: IpAccount[];
  agents: AgentUse[];
  agents_sample: number;
  requests_24h: number;
  requests_24h_capped: boolean;
  labels: Labels;
};
