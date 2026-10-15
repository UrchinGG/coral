import { describe, expect, it } from "vitest";

import { ApiError } from "../api/http";
import { omniboxStatus, type SearchProgress } from "./status";

const SETTLED: SearchProgress = { text: "Kestrel_", current: true, fetching: false, error: null, count: 3 };

function apiError(status: number): ApiError {
  return new ApiError({ status, code: `http_${status}`, message: "failed", endpoint: "POST /api/search", serverMs: null, serverTime: null });
}

describe("omniboxStatus", () => {
  it("is idle when only an operator or quotes were typed", () => {
    expect(omniboxStatus({ ...SETTLED, text: "", current: false })).toEqual({ kind: "idle" });
  });

  it("is searching until the debounced text matches the input", () => {
    expect(omniboxStatus({ ...SETTLED, current: false })).toEqual({ kind: "searching" });
  });

  it("is searching while a local or remote search is in flight", () => {
    expect(omniboxStatus({ ...SETTLED, fetching: true })).toEqual({ kind: "searching" });
  });

  it("reports a missing search endpoint as unavailable", () => {
    const error = apiError(404);
    expect(omniboxStatus({ ...SETTLED, error })).toEqual({ kind: "unavailable", error });
  });

  it("reports any other failure", () => {
    const error = apiError(500);
    expect(omniboxStatus({ ...SETTLED, error })).toEqual({ kind: "failed", error });
  });

  it("is ready with the hit count once settled", () => {
    expect(omniboxStatus(SETTLED)).toEqual({ kind: "ready", count: 3 });
  });
});
