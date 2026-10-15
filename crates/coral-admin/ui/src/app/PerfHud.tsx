import { isOverBudget, serverMsOf, slowestOf, usePageQueries, type PageQuery } from "../api/page-queries";
import { Popover } from "../kit/Popover";
import { cx } from "../lib/cx";
import { formatNumber } from "../lib/format";

export function PerfHud() {
  const queries = usePageQueries();
  const slowest = slowestOf(queries);
  const slowestMs = serverMsOf(slowest);
  const overBudget = queries.filter(isOverBudget).length;
  const sorted = [...queries].sort((a, b) => serverMsOf(b) - serverMsOf(a));
  return (
    <Popover
      label={`${queries.length} queries on this page, slowest ${slowestMs < 0 ? "not timed yet" : `${Math.round(slowestMs)} milliseconds`}, ${overBudget} over budget. Show details.`}
      buttonClassName="inline-flex h-7 items-center gap-1.5 rounded-md border border-border bg-surface-2 px-2.5 text-12 whitespace-nowrap text-muted hover:text-fg max-md:hidden"
      panelClassName="w-[min(380px,calc(100vw-32px))] py-2.5 pb-1.5"
      button={
        <>
          <b className="font-semibold text-fg">{queries.length}</b> queries · slowest{" "}
          <b className={cx("font-semibold", slowest !== null && isOverBudget(slowest) ? "text-warning" : "text-fg")}>{slowestMs < 0 ? "—" : `${Math.round(slowestMs)} ms`}</b>
          {overBudget > 0 && <span className="text-warning">· {overBudget} over</span>}
        </>
      }
    >
      {() => (
        <div>
          <div className="border-b border-border px-3 pb-2 text-12 text-muted">
            Queries on this page, from Server-Timing, against each endpoint's p95 budget.
          </div>
          {sorted.length === 0 && <div className="px-3 py-2 text-12 text-muted">No queries are mounted on this page.</div>}
          {sorted.map((query) => (
            <QueryRow key={query.hash} query={query} />
          ))}
        </div>
      )}
    </Popover>
  );
}

function QueryRow({ query }: { query: PageQuery }) {
  const ms = serverMsOf(query);
  return (
    <div className="grid grid-cols-[minmax(0,1fr)_auto] gap-2.5 px-3 py-[5px] text-12">
      <span className="font-mono wrap-anywhere">
        {query.label}
        {query.errorCode !== null && <span className="ml-1.5 text-danger">{query.errorCode}</span>}
        {query.fetching && <span className="ml-1.5 text-subtle">fetching</span>}
      </span>
      <span className={cx("text-right font-mono whitespace-nowrap", isOverBudget(query) && "text-warning")}>
        {ms < 0 ? "—" : `${formatNumber(ms)} ms`}
        {query.budgetMs !== null && <span className="text-subtle"> / {query.budgetMs}</span>}
      </span>
    </div>
  );
}
