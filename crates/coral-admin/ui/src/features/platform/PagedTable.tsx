import type { InfiniteData, UseInfiniteQueryResult } from "@tanstack/react-query";
import type { ReactNode } from "react";

import { rowsOf } from "../../api/row-pages";
import type { RowPage } from "../../api/types/platform";
import { Section } from "../../kit/Section";
import { Table, type Column } from "../../kit/Table";

type PagedTableProps<T> = {
  title: ReactNode;
  label: string;
  query: UseInfiniteQueryResult<InfiniteData<RowPage<T>>, Error>;
  columns: readonly Column<T>[];
  rowKey: (row: T) => string;
  empty: ReactNode;
  actions?: ReactNode;
  onRowClick?: (row: T) => void;
};

export function PagedTable<T>({ title, label, query, columns, rowKey, empty, actions, onRowClick }: PagedTableProps<T>) {
  const rows = rowsOf(query.data?.pages);
  const refetching = query.isFetching && !query.isFetchingNextPage && query.data !== undefined;
  const hasMore = query.hasNextPage && !query.isPlaceholderData;
  return (
    <Section title={title} count={query.data === undefined ? undefined : `${rows.length}${hasMore ? "+" : ""}`} actions={actions} flush fetching={refetching}>
      <Table
        label={label}
        columns={columns}
        rows={rows}
        rowKey={rowKey}
        onRowClick={onRowClick}
        loading={query.isPending || (query.isPlaceholderData && rows.length === 0)}
        error={query.error}
        onRetry={() => void query.refetch()}
        empty={empty}
        infinite={{ hasMore, loading: query.isFetchingNextPage, onLoadMore: () => void query.fetchNextPage() }}
      />
    </Section>
  );
}
