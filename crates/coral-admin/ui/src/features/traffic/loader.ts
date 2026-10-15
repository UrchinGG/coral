import type { LoaderFunctionArgs } from "react-router-dom";

import { queryClient } from "../../api/queryClient";
import { trafficSummaryQuery } from "../../api/traffic";
import { callersQuery, pressureQuery, routesQuery, seriesQuery, upstreamQuery } from "../../api/traffic-panels";
import { trafficViewOf } from "./traffic-view";

export function trafficLoader({ request }: LoaderFunctionArgs): null {
  const view = trafficViewOf(new URL(request.url).searchParams);
  void queryClient.prefetchQuery(trafficSummaryQuery(view.scope, view.compare));
  void queryClient.prefetchQuery(seriesQuery(view.chart));
  void queryClient.prefetchQuery(routesQuery(view.scope));
  void queryClient.prefetchQuery(callersQuery(view.scope, view.callers));
  void queryClient.prefetchQuery(pressureQuery());
  void queryClient.prefetchQuery(upstreamQuery(view.scope));
  return null;
}
