export interface ApiKey {
  key: string;
  permissions: string[] | null;
  used: number;
  limit: number;
  total: number;
  created_at: string;
  locked: boolean;
}

export interface Keys {
  personal: ApiKey | null;
  developer: ApiKey | null;
}

export interface Starfish {
  device: { registered_at: string; last_seen: string | null } | null;
  changes_left: number;
}

export type LentStatus =
  "working" | "idle" | "paused" | "finishing" | "signed-out" | "banned";

export interface LentAccount {
  uuid: string;
  name: string;
  rank: string | null;
  status: LentStatus;
  task: string | null;
  paused_until: string | null;
  added_at: string;
  sign_in_code: string | null;
}

export interface LentSignIn {
  id: string;
  code: string;
}

export interface LentSignInResult {
  account: LentAccount | null;
}
