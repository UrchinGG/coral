import { recentActionsQuery } from "../../api/actions";
import { flagsQuery } from "../../api/flags";
import { recentModerationQuery } from "../../api/moderation";
import { queryClient } from "../../api/queryClient";
import { pulseQuery } from "../../api/traffic";
import { pulseSeriesQuery, pulseUpstreamQuery } from "../../api/traffic-panels";

export function homeLoader(): null {
  void queryClient.prefetchQuery(pulseQuery());
  void queryClient.prefetchQuery(pulseSeriesQuery());
  void queryClient.prefetchQuery(pulseUpstreamQuery());
  void queryClient.prefetchQuery(flagsQuery());
  void queryClient.prefetchQuery(recentActionsQuery());
  void queryClient.prefetchQuery(recentModerationQuery());
  return null;
}
