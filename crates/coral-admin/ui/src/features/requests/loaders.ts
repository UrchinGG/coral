import type { LoaderFunctionArgs } from "react-router-dom";

import { queryClient } from "../../api/queryClient";
import { requestFullQuery } from "../../api/requests";

export function requestLoader({ params, request }: LoaderFunctionArgs): null {
  const requestId = params.requestId;
  if (requestId !== undefined) void queryClient.prefetchQuery(requestFullQuery(requestId, new URL(request.url).searchParams.get("t")));
  return null;
}
