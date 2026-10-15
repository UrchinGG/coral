import { useIsFetching } from "@tanstack/react-query";

import { useLabelFailure } from "../api/labels";
import { serverMsOf, slowestOf, usePageQueries } from "../api/page-queries";
import { cx } from "../lib/cx";
import { formatNumber } from "../lib/format";
import { readWriter } from "../lib/writer";
import { useSession } from "./boot";

export function StatusBar() {
  const session = useSession();
  const queries = usePageQueries();
  const fetching = useIsFetching();
  const labelFailure = useLabelFailure();
  const slowest = slowestOf(queries);
  const writer = readWriter(session.coverage.writer);
  return (
    <footer className="fixed inset-x-0 bottom-0 z-45 flex h-[calc(var(--spacing-statusbar)+env(safe-area-inset-bottom,0px))] items-center gap-3.5 overflow-hidden border-t border-border bg-surface px-3 pb-[env(safe-area-inset-bottom,0px)] text-11 whitespace-nowrap text-subtle">
      <span>
        build <span className="font-mono text-muted">{session.build ?? "unset"}</span>
      </span>
      <Separator className="max-md:hidden" />
      <span className="min-w-0 truncate max-md:hidden">
        {slowest === null || serverMsOf(slowest) < 0 ? (
          "no timed queries yet"
        ) : (
          <>
            slowest <span className="font-mono text-muted">{slowest.label}</span> {formatNumber(serverMsOf(slowest))} ms
          </>
        )}
      </span>
      <Separator className="max-md:hidden" />
      <span className="max-md:hidden">{fetching > 0 ? `fetching ${fetching}…` : "idle"}</span>
      <Separator />
      <span className="inline-flex min-w-0 items-center gap-1.5">
        <span aria-hidden className={cx("size-1.5 shrink-0 rounded-full", writer.healthy ? "bg-ok" : "bg-warning")} />
        <span className="truncate">{writer.text}</span>
      </span>
      {labelFailure !== null && (
        <>
          <Separator />
          <span className="truncate text-warning" title={labelFailure.error.message}>
            names unavailable: {labelFailure.error.message}
          </span>
        </>
      )}
    </footer>
  );
}

function Separator({ className }: { className?: string }) {
  return <span aria-hidden className={cx("h-3 w-px shrink-0 bg-border-strong", className)} />;
}
