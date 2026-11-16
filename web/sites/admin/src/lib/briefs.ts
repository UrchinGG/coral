import { load } from "@/lib/api";
import type { Labels } from "@/lib/members";
import type { Ref } from "@/lib/refs";

export interface Brief {
  player: string;
  tag: { tag_type: string; display_name: string } | null;
  owner: Ref | null;
  last_seen: string | null;
}

export interface Briefs {
  players: Brief[];
  labels: Labels;
}

export const loadBriefs = (players: string[]) =>
  load<Briefs>(`/players/briefs?players=${players.join(",")}`);
