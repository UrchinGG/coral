"use client";

import Image from "next/image";
import { useState } from "react";
import type { PingRecord } from "@/lib/utils/ping";
import { relativeTime } from "@/lib/utils/general/format";
import { getPingIcon } from "@/lib/utils/ping";
import { PillShell } from "@/components/panels/shells/PillShell";
import { TooltipShell } from "@/components/panels/shells/TooltipShell";

export function PingPill({ latest }: { latest?: PingRecord }) {
  const [hover, setHover] = useState(false);
  const MIN_WIDTH = 180;
  const MAX_WIDTH = 360;
  const icon = getPingIcon(latest?.avg);
  const subtitle = latest
    ? `Last pinged ${relativeTime(latest.timestamp)}`
    : undefined;

  return (
    <div
      className="mt-1 relative inline-flex items-center"
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
    >
      <PillShell>
        <Image src={icon} alt="Ping" width={16} height={16} />
        <span
          className="font-semibold"
          style={{ fontFamily: "var(--font-inter)" }}
        >
          {latest ? `${latest.avg}ms` : "Ping"}
        </span>
      </PillShell>
      <TooltipShell hover={hover} minWidth={MIN_WIDTH} maxWidth={MAX_WIDTH}>
        {latest ? (
          <div className="space-y-1">
            <div className="whitespace-nowrap">
              <span className="font-semibold">Average:</span> {latest.avg}ms
            </div>
            <div className="whitespace-nowrap">
              <span className="font-semibold">Minimum:</span> {latest.min}ms
            </div>
            <div className="whitespace-nowrap">
              <span className="font-semibold">Maximum:</span> {latest.max}ms
            </div>
            {subtitle ? (
              <div className="opacity-75 mt-1 whitespace-nowrap">
                {subtitle}
              </div>
            ) : null}
          </div>
        ) : (
          <div>No ping data.</div>
        )}
      </TooltipShell>
    </div>
  );
}
