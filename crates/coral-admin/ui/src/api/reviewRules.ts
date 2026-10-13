import { useQuery } from "@tanstack/react-query";
import { apiGet, apiPut } from "./client";
import { useAdminMutation } from "./useAdminMutation";

export type ReviewRules = {
  accept_threshold: number;
  reject_threshold: number;
  vote_grant_approvals: number;
  tag_grant_correct: number;
  earn_ratio: number;
  bonus_weight: number;
  revoke_strikes: number;
};

export type ReviewRulesResponse = {
  rules: ReviewRules;
  defaults: ReviewRules;
};

export type ReviewRulesUpdate = {
  rules: ReviewRules;
  vote_changes: number;
  tag_changes: number;
};

const rulesKeys = [["review-rules"]];

export function useReviewRules() {
  return useQuery({
    queryKey: ["review-rules"],
    queryFn: () => apiGet<ReviewRulesResponse>("/review-rules"),
  });
}

export function useSaveReviewRules() {
  return useAdminMutation((rules: ReviewRules) => apiPut<ReviewRulesUpdate>("/review-rules", rules), {
    successMessage: "Review rules saved",
    invalidateKeys: rulesKeys,
  });
}
