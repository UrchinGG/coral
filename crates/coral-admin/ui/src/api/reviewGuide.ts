import { useQuery } from "@tanstack/react-query";
import { apiGet, apiPost, apiPut } from "./client";
import type { GuideContent, ReviewGuideResponse } from "./types";
import { useAdminMutation } from "./useAdminMutation";

const guideKeys = [["review-guide"]];

export function useReviewGuide() {
  return useQuery({
    queryKey: ["review-guide"],
    queryFn: () => apiGet<ReviewGuideResponse>("/review-guide"),
  });
}

export function useSaveGuideContent() {
  return useAdminMutation((content: GuideContent) => apiPut<void>("/review-guide", { content }), {
    successMessage: "Guide content saved",
    invalidateKeys: guideKeys,
  });
}

export function useSetPingRoles() {
  return useAdminMutation(
    (config: { review_role_id: string | null; dispute_role_id: string | null }) =>
      apiPut<void>("/review-guide", { ping_roles: config }),
    {
      successMessage: "Ping roles updated",
      invalidateKeys: guideKeys,
    },
  );
}

export function usePostGuide() {
  return useAdminMutation<void, void>(() => apiPost<void>("/review-guide/publish"), {
    successMessage: "Guide posted and pinned",
    invalidateKeys: guideKeys,
  });
}
