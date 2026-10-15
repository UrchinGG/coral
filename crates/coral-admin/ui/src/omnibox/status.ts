import { isApiError, type ApiError } from "../api/http";

export type OmniboxStatus =
  | { kind: "idle" }
  | { kind: "searching" }
  | { kind: "ready"; count: number }
  | { kind: "unavailable"; error: ApiError }
  | { kind: "failed"; error: Error };

export type SearchProgress = {
  text: string;
  current: boolean;
  fetching: boolean;
  error: Error | null;
  count: number;
};

const NOT_FOUND = 404;

export function omniboxStatus({ text, current, fetching, error, count }: SearchProgress): OmniboxStatus {
  if (text === "") return { kind: "idle" };
  if (!current || fetching) return { kind: "searching" };
  if (isApiError(error) && error.status === NOT_FOUND) return { kind: "unavailable", error };
  if (error !== null) return { kind: "failed", error };
  return { kind: "ready", count };
}
