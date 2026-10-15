import type { LoaderFunctionArgs } from "react-router-dom";

import { ipCoreQuery, ipRequestsQuery } from "../../api/ips";
import { queryClient } from "../../api/queryClient";

export function ipLoader({ params }: LoaderFunctionArgs): null {
  const ip = params.ip ?? "";
  void queryClient.prefetchQuery(ipCoreQuery(ip));
  void queryClient.prefetchInfiniteQuery(ipRequestsQuery(ip));
  return null;
}
