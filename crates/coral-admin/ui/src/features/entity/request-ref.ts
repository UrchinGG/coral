import type { RequestRef } from "../../api/types/refs";
import type { RequestRow } from "../../api/types/requests";

export function requestRef(row: Pick<RequestRow, "request_id" | "ts">): RequestRef {
  return { kind: "request", request_id: row.request_id, ts: row.ts };
}
