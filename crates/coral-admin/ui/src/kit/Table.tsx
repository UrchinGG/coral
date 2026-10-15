import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useRef, type KeyboardEvent, type MouseEvent, type ReactNode, type Ref, type RefObject } from "react";

import { cx } from "../lib/cx";
import { EmptyState, ErrorState, SkeletonRows } from "./States";

export type Column<T> = {
  id: string;
  header: ReactNode;
  cell: (row: T) => ReactNode;
  numeric?: boolean;
  priority?: 1 | 2 | 3;
  className?: string;
};

export type InfiniteRows = {
  hasMore: boolean;
  loading: boolean;
  onLoadMore: () => void;
};

type TableProps<T> = {
  label: string;
  columns: readonly Column<T>[];
  rows: readonly T[];
  rowKey: (row: T) => string;
  selectedKey?: string | null;
  onRowClick?: (row: T) => void;
  height?: string;
  virtual?: boolean;
  infinite?: InfiniteRows;
  loading?: boolean;
  error?: unknown;
  onRetry?: () => void;
  empty?: ReactNode;
  footer?: ReactNode;
};

type GridProps<T> = {
  label: string;
  columns: readonly Column<T>[];
  rows: readonly T[];
  rowKey: (row: T) => string;
  selectedKey: string | null;
  onRowClick?: (row: T) => void;
  maxHeight?: string;
  infinite?: InfiniteRows;
};

type TableRowProps<T> = {
  row: T;
  columns: readonly Column<T>[];
  selected: boolean;
  onRowClick?: (row: T) => void;
  index?: number;
  rowRef?: Ref<HTMLTableRowElement>;
};

export const ROW_HEIGHT_PX = 28;
const OVERSCAN_ROWS = 20;
const LOAD_AHEAD_SCREENS = 2;
const DEFAULT_VIRTUAL_HEIGHT = "560px";
const INTERACTIVE_SELECTOR = "a, button, input, select, textarea, summary, [role=button]";

export function Table<T>({
  label,
  columns,
  rows,
  rowKey,
  selectedKey = null,
  onRowClick,
  height,
  virtual = false,
  infinite,
  loading = false,
  error,
  onRetry,
  empty,
  footer,
}: TableProps<T>) {
  const failed = error !== undefined && error !== null;

  if (rows.length === 0) {
    if (failed) return <ErrorState error={error} onRetry={onRetry} />;
    if (loading) return <SkeletonRows />;
    return <EmptyState>{empty ?? "No rows."}</EmptyState>;
  }

  const grid: GridProps<T> = { label, columns, rows, rowKey, selectedKey, onRowClick, infinite };
  return (
    <div className="min-w-0">
      {failed && (
        <div className="p-2">
          <ErrorState error={error} onRetry={onRetry} />
        </div>
      )}
      {virtual ? <VirtualGrid {...grid} maxHeight={height ?? DEFAULT_VIRTUAL_HEIGHT} /> : <PlainGrid {...grid} maxHeight={height} />}
      <TableFooter infinite={infinite}>{footer}</TableFooter>
    </div>
  );
}

function PlainGrid<T>({ label, columns, rows, rowKey, selectedKey, onRowClick, maxHeight, infinite }: GridProps<T>) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const selectedRef = useRef<HTMLTableRowElement>(null);

  useEffect(() => {
    selectedRef.current?.scrollIntoView({ block: "nearest" });
  }, [selectedKey]);

  return (
    <div ref={scrollRef} className="max-w-full overflow-auto" style={maxHeight === undefined ? undefined : { maxHeight }}>
      <table aria-label={label} className="w-full border-separate border-spacing-0 text-12">
        <TableHead columns={columns} />
        <tbody>
          {rows.map((row) => {
            const key = rowKey(row);
            const selected = key === selectedKey;
            return (
              <TableRow
                key={key}
                row={row}
                columns={columns}
                selected={selected}
                onRowClick={onRowClick}
                rowRef={selected ? selectedRef : undefined}
              />
            );
          })}
        </tbody>
      </table>
      {infinite !== undefined && <LoadMoreSentinel infinite={infinite} scrollRef={scrollRef} scrolls={maxHeight !== undefined} />}
    </div>
  );
}

function VirtualGrid<T>({ label, columns, rows, rowKey, selectedKey, onRowClick, maxHeight, infinite }: GridProps<T>) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const scrolledTo = useRef<string | null>(null);
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT_PX,
    overscan: OVERSCAN_ROWS,
    getItemKey: (index) => rowKey(rows[index]),
  });

  useEffect(() => {
    if (selectedKey === null || selectedKey === scrolledTo.current) return;
    const index = rows.findIndex((row) => rowKey(row) === selectedKey);
    if (index < 0) return;
    virtualizer.scrollToIndex(index, { align: "auto" });
    scrolledTo.current = selectedKey;
  }, [selectedKey, rows, rowKey, virtualizer]);

  const items = virtualizer.getVirtualItems();
  const paddingTop = items.length > 0 ? items[0].start : 0;
  const paddingBottom = items.length > 0 ? virtualizer.getTotalSize() - items[items.length - 1].end : 0;

  return (
    <div ref={scrollRef} className="max-w-full overflow-auto" style={{ maxHeight }}>
      <table aria-label={label} aria-rowcount={rows.length} className="w-full border-separate border-spacing-0 text-12">
        <TableHead columns={columns} />
        <tbody>
          {paddingTop > 0 && <SpacerRow height={paddingTop} span={columns.length} />}
          {items.map((item) => {
            const row = rows[item.index];
            const key = rowKey(row);
            return (
              <TableRow
                key={key}
                row={row}
                columns={columns}
                selected={key === selectedKey}
                onRowClick={onRowClick}
                index={item.index}
                rowRef={virtualizer.measureElement}
              />
            );
          })}
          {paddingBottom > 0 && <SpacerRow height={paddingBottom} span={columns.length} />}
        </tbody>
      </table>
      {infinite !== undefined && <LoadMoreSentinel infinite={infinite} scrollRef={scrollRef} scrolls />}
    </div>
  );
}

function TableHead<T>({ columns }: { columns: readonly Column<T>[] }) {
  return (
    <thead>
      <tr>
        {columns.map((column) => (
          <th
            key={column.id}
            scope="col"
            className={cx(
              "sticky top-0 z-[1] h-[30px] border-b border-border bg-surface px-2.5 text-left text-11 font-semibold tracking-[0.05em] whitespace-nowrap text-subtle uppercase",
              column.numeric && "text-right",
              priorityClass(column.priority),
            )}
          >
            {column.header}
          </th>
        ))}
      </tr>
    </thead>
  );
}

function TableRow<T>({ row, columns, selected, onRowClick, index, rowRef }: TableRowProps<T>) {
  const clickable = onRowClick !== undefined;
  const onClick = (event: MouseEvent<HTMLTableRowElement>) => {
    if (!clickable || (event.target instanceof Element && event.target.closest(INTERACTIVE_SELECTOR))) return;
    onRowClick(row);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLTableRowElement>) => {
    if (!clickable || event.key !== "Enter" || event.target !== event.currentTarget) return;
    event.preventDefault();
    onRowClick(row);
  };
  return (
    <tr
      ref={rowRef}
      data-index={index}
      aria-rowindex={index === undefined ? undefined : index + 1}
      aria-selected={clickable ? selected : undefined}
      tabIndex={clickable ? 0 : undefined}
      onClick={onClick}
      onKeyDown={onKeyDown}
      className={cx("group/row", clickable && "cursor-pointer")}
    >
      {columns.map((column, columnIndex) => (
        <td
          key={column.id}
          className={cx(
            "h-row border-b border-border px-2.5 py-1 align-middle group-last/row:border-b-0 group-hover/row:bg-white/[0.018]",
            selected && "bg-accent/14 group-hover/row:bg-accent/14",
            selected && columnIndex === 0 && "shadow-[inset_2px_0_0_var(--color-accent)]",
            column.numeric && "text-right whitespace-nowrap",
            priorityClass(column.priority),
            column.className,
          )}
        >
          {column.cell(row)}
        </td>
      ))}
    </tr>
  );
}

function SpacerRow({ height, span }: { height: number; span: number }) {
  return (
    <tr aria-hidden>
      <td colSpan={span} className="border-0 p-0" style={{ height }} />
    </tr>
  );
}

function LoadMoreSentinel({ infinite, scrollRef, scrolls }: { infinite: InfiniteRows; scrollRef: RefObject<HTMLDivElement | null>; scrolls: boolean }) {
  const sentinelRef = useRef<HTMLDivElement>(null);
  const { hasMore, loading, onLoadMore } = infinite;

  useEffect(() => {
    const sentinel = sentinelRef.current;
    if (sentinel === null || !hasMore || loading) return;
    const observer = new IntersectionObserver((entries) => entries.some((entry) => entry.isIntersecting) && onLoadMore(), {
      root: scrolls ? scrollRef.current : null,
      rootMargin: `${LOAD_AHEAD_SCREENS * 100}% 0px`,
    });
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [hasMore, loading, onLoadMore, scrollRef, scrolls]);

  return <div ref={sentinelRef} aria-hidden className="h-px" />;
}

function TableFooter({ infinite, children }: { infinite?: InfiniteRows; children?: ReactNode }) {
  const status = infinite === undefined ? null : infinite.loading ? "Loading more…" : infinite.hasMore ? null : "End of results";
  if (status === null && children === undefined) return null;
  return (
    <div className="flex flex-wrap items-center justify-between gap-2.5 border-t border-border px-3 py-2 text-12 text-subtle">
      <span>{children}</span>
      {status !== null && <span>{status}</span>}
    </div>
  );
}

function priorityClass(priority: Column<unknown>["priority"]): string | undefined {
  return priority !== undefined && priority > 1 ? "max-md:hidden" : undefined;
}
