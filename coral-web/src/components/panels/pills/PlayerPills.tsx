"use client";

import { useEffect, useMemo, useState } from "react";
import { PingPill } from "@/components/panels/pills/PingPill";
import { UrchinTagPill } from "@/components/panels/pills/UrchinTagPill";
import type { PingRecord } from "@/lib/utils/ping";
import type { UrchinResponse, UrchinTag } from "@/lib/utils/urchin";
import { getLatestPing } from "@/lib/utils/ping";
import { getUrchinLabel, sortUrchinTags } from "@/lib/utils/urchin";

export function PlayerPills() {
  const [latestPing, setLatestPing] = useState<PingRecord | undefined>();
  const [urchin, setUrchin] = useState<UrchinResponse | null>(null);
  const [uuid, setUuid] = useState<string | null>(null);

  useEffect(() => {
    try {
      const el = document.querySelector("meta[name='coral:player:uuid']");
      const id = el?.getAttribute("content");
      if (id) setUuid(id);
    } catch {}
  }, []);

  useEffect(() => {
    if (!uuid) return;
    let cancelled = false;
    (async () => {
      try {
        const res = await fetch(`/api/ping?uuid=${encodeURIComponent(uuid)}`);
        if (!res.ok) return;
        const json = (await res.json()) as { data?: PingRecord[] };
        if (!cancelled) setLatestPing(getLatestPing(json?.data || []));
      } catch {}
    })();
    return () => {
      cancelled = true;
    };
  }, [uuid]);

  useEffect(() => {
    if (!uuid) return;
    let cancelled = false;
    (async () => {
      try {
        const res = await fetch(`/api/urchin?uuid=${encodeURIComponent(uuid)}`);
        if (!res.ok) return;
        const json = (await res.json()) as UrchinResponse;
        if (!cancelled) setUrchin(json ?? null);
      } catch {}
    })();
    return () => {
      cancelled = true;
    };
  }, [uuid]);

  const sortedTags: UrchinTag[] = useMemo(
    () => (urchin?.tags ? sortUrchinTags(urchin.tags) : []),
    [urchin]
  );

  return (
    <>
      {typeof latestPing !== "undefined" ? (
        <PingPill latest={latestPing} />
      ) : null}
      {sortedTags.map((t, i) => (
        <UrchinTagPill
          key={`${t.type}-${i}`}
          type={t.type}
          label={getUrchinLabel(t.type) ?? "Tag"}
          reason={t.reason ?? undefined}
          addedBy={t.added_by_username ?? (t.added_by_id != null ? String(t.added_by_id) : undefined)}
          addedOn={t.added_on ?? undefined}
        />
      ))}
    </>
  );
}
