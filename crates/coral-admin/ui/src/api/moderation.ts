import { queryOptions } from "@tanstack/react-query";

import { getJson, postJson, putJson } from "./http";
import { keys } from "./keys";
import { withLabels } from "./labels";
import { configQuery, getQuery, immutableQuery, liveQuery } from "./queryClient";
import { rowPages } from "./row-pages";
import type {
  AuditRow,
  BlacklistFilters,
  BlacklistRow,
  Guide,
  GuideContent,
  PingRoles,
  ReviewRules,
  RulesUpdate,
  RulesView,
  StrikeFilter,
  StrikeRow,
  TagType,
} from "./types/platform";
import type { RecentModeration } from "./types/moderation";

export const POLL_MODERATION_MS = 60_000;

export function tagTypesQuery() {
  return queryOptions({ queryKey: keys.moderation.tagTypes(), queryFn: getQuery<TagType[]>("/moderation/tag-types"), ...immutableQuery });
}

export function recentModerationQuery() {
  return queryOptions({
    queryKey: keys.moderation.recent(),
    queryFn: getQuery<RecentModeration>("/moderation/recent"),
    ...liveQuery(POLL_MODERATION_MS),
  });
}

export function blacklistQuery(filters: BlacklistFilters) {
  const params = new URLSearchParams({ field: filters.field, dir: filters.dir });
  if (filters.q !== "") params.set("q", filters.q);
  if (filters.tag !== "") params.set("tag", filters.tag);
  return rowPages<BlacklistRow>(keys.moderation.blacklist(filters), "/moderation/blacklist", params);
}

export function strikesQuery(state: StrikeFilter) {
  return rowPages<StrikeRow>(keys.moderation.strikes(state), "/moderation/strikes", new URLSearchParams({ state }));
}

export function auditQuery(action: string) {
  const params = new URLSearchParams();
  if (action !== "") params.set("action", action);
  return rowPages<AuditRow>(keys.moderation.audit(action), "/moderation/audit", params);
}

export function guideQuery() {
  return queryOptions({
    queryKey: keys.moderation.guide(),
    queryFn: ({ signal, queryKey }) => getJson<Guide>("/moderation/guide", { signal, timingKey: queryKey }).then(withLabels),
    ...configQuery,
  });
}

export function rulesQuery() {
  return queryOptions({ queryKey: keys.moderation.rules(), queryFn: getQuery<RulesView>("/moderation/rules"), ...configQuery });
}

export function saveGuide(content: GuideContent): Promise<void> {
  return putJson<void>("/moderation/guide", content);
}

export function savePingRoles(roles: PingRoles): Promise<void> {
  return putJson<void>("/moderation/guide/ping-roles", roles);
}

export function publishGuide(): Promise<void> {
  return postJson<void>("/moderation/guide/publish", {});
}

export function saveRules(rules: ReviewRules): Promise<RulesUpdate> {
  return putJson<RulesUpdate>("/moderation/rules", rules);
}
