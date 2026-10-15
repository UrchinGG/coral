import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import { Navigate, useParams } from "react-router-dom";

import { memberAccountQuery, starfishAccountQuery } from "../../api/accounts";
import { QueryContent } from "../../kit/QueryContent";
import type { AccountTab } from "./vocabulary";

const STARFISH_TAB: AccountTab = "starfish";

export function MemberRedirect() {
  const { memberId = "" } = useParams();
  return <ResolvedRedirect resolved={useQuery(memberAccountQuery(memberId))} tab={null} />;
}

export function StarfishRedirect() {
  const { userId = "" } = useParams();
  return <ResolvedRedirect resolved={useQuery(starfishAccountQuery(userId))} tab={STARFISH_TAB} />;
}

function ResolvedRedirect({ resolved, tab }: { resolved: UseQueryResult<{ discord_id: string }, Error>; tab: AccountTab | null }) {
  return (
    <QueryContent query={resolved}>
      {(data) => <Navigate to={`/accounts/${data.discord_id}${tab === null ? "" : `?${new URLSearchParams({ tab })}`}`} replace />}
    </QueryContent>
  );
}
