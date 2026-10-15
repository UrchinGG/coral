import type { UseQueryResult } from "@tanstack/react-query";
import type { ReactNode } from "react";

import { EmptyState, ErrorState, SkeletonLines } from "./States";

type QueryContentProps<T> = {
  query: UseQueryResult<T, Error>;
  skeleton?: ReactNode;
  isEmpty?: (data: T) => boolean;
  empty?: ReactNode;
  children: (data: T) => ReactNode;
};

export function QueryContent<T>({ query, skeleton, isEmpty, empty, children }: QueryContentProps<T>) {
  const retry = () => void query.refetch();
  if (query.data === undefined) {
    if (query.error !== null) return <ErrorState error={query.error} onRetry={retry} retrying={query.isFetching} />;
    return <>{skeleton ?? <SkeletonLines />}</>;
  }
  const content = isEmpty?.(query.data) ? <EmptyState>{empty ?? "Nothing here."}</EmptyState> : children(query.data);
  if (query.error === null) return <>{content}</>;
  return (
    <div className="flex flex-col gap-2">
      <ErrorState error={query.error} onRetry={retry} retrying={query.isFetching} title="Refresh failed; showing the last good data" />
      {content}
    </div>
  );
}
