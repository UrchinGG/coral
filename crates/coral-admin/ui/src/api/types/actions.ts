import type { EntityRef } from "./refs";

export type AdminAction = {
  id: number;
  at: string;
  actor: EntityRef;
  action: string;
  target: EntityRef | null;
  target_text: string;
  details: unknown;
};

export type AdminActionPage = { items: AdminAction[]; next_before: string | null };
