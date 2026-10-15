import { useEffect, useRef, useState } from "react";

import { requestTail } from "../../api/requests";
import { POLL_TAIL_MS } from "../../api/queryClient";
import type { RequestQuery } from "../../api/types/request-explorer";
import type { RequestRow } from "../../api/types/requests";
import { cursorOf } from "./scan";

export type RequestTail = { rows: RequestRow[]; error: Error | null };

export type TailSeed = Pick<RequestRow, "ts" | "request_id">;

type TailState = RequestTail & { signature: string };

const IDLE_TAIL: RequestTail = { rows: [], error: null };

export function useRequestTail(query: RequestQuery, enabled: boolean, seed: TailSeed | null): RequestTail {
  const signature = JSON.stringify(query);
  const [tail, setTail] = useState<TailState>({ ...IDLE_TAIL, signature });
  const current = tail.signature === signature ? tail : { ...IDLE_TAIL, signature };
  if (current !== tail) setTail(current);
  const seedRef = useRef(seed);
  const queryRef = useRef(query);
  const rowsRef = useRef(current.rows);

  useEffect(() => {
    seedRef.current = seed;
    queryRef.current = query;
    rowsRef.current = current.rows;
  });

  useEffect(() => {
    if (!enabled) return;
    const controller = new AbortController();
    let newest: TailSeed | null = rowsRef.current[0] ?? null;
    let timer = 0;
    const poll = async () => {
      const after = newest ?? seedRef.current;
      if (document.visibilityState !== "visible" || after === null) {
        timer = window.setTimeout(poll, POLL_TAIL_MS);
        return;
      }
      try {
        const page = await requestTail(queryRef.current, cursorOf(after), controller.signal);
        if (controller.signal.aborted) return;
        newest = page.items[0] ?? after;
        setTail((state) => ({ signature: state.signature, error: null, rows: [...page.items, ...state.rows] }));
        timer = window.setTimeout(poll, page.next_cursor === null ? POLL_TAIL_MS : 0);
      } catch (error) {
        if (controller.signal.aborted) return;
        setTail((state) => ({ ...state, error: error instanceof Error ? error : new Error(String(error)) }));
        timer = window.setTimeout(poll, POLL_TAIL_MS);
      }
    };
    timer = window.setTimeout(poll, POLL_TAIL_MS);
    return () => {
      controller.abort();
      window.clearTimeout(timer);
    };
  }, [enabled, signature]);

  return current;
}
