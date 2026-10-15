import type { RequestRow } from "../../api/types/requests";

export const REQUEST_TEXT_STATE_KEY = "q";

export function requestPath(row: Pick<RequestRow, "request_id" | "ts">): string {
  return `/requests/${row.request_id}?${new URLSearchParams({ t: row.ts })}`;
}
