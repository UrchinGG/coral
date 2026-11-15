import type { Labels } from "@/lib/members";
import type { Ref } from "@/lib/refs";
import type { Directory, Population } from "@/lib/server/common";

export interface PingRoles {
  review_role_id: string | null;
  dispute_role_id: string | null;
  confirm_role_id: string | null;
}

export interface Guide {
  content: { body: string };
  status: {
    posted: boolean;
    forum_channel_id: string | null;
    thread_id: string | null;
    posted_at: string | null;
    posted_by: Ref | null;
    content_updated_at: string;
    up_to_date: boolean;
  };
  ping_roles: PingRoles;
  labels: Labels;
}

export interface Reviews {
  guide: Guide;
  directory: Directory | null;
  population: Population | null;
  refresh: () => void;
  report: (text: string, bad?: boolean) => void;
}
