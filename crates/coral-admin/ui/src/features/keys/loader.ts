import type { LoaderFunctionArgs } from "react-router-dom";

import { keyByIdQuery, keyByPrefixQuery, keyRequestsQuery } from "../../api/apiKeys";
import { queryClient } from "../../api/queryClient";
import { keyRouteOf } from "./key-route";

export function keyLoader({ params }: LoaderFunctionArgs): null {
  const route = keyRouteOf(params["*"] ?? "");
  if (route.kind === "id") {
    void queryClient.prefetchQuery(keyByIdQuery(route.keyId));
    return null;
  }
  void queryClient.prefetchQuery(keyByPrefixQuery(route.prefix));
  void queryClient.prefetchInfiniteQuery(keyRequestsQuery(route.prefix));
  return null;
}
