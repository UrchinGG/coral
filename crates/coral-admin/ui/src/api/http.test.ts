import { hashKey } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ApiError, getJson, postJson } from "./http";
import { timingOf } from "./timing";

const assign = vi.fn();
const fetchMock = vi.fn<typeof fetch>();

function jsonResponse(body: unknown, init: ResponseInit = {}): Response {
  return new Response(JSON.stringify(body), { status: 200, headers: { "content-type": "application/json" }, ...init });
}

async function failureOf(promise: Promise<unknown>): Promise<unknown> {
  return promise.then(
    () => {
      throw new Error("expected the request to fail");
    },
    (error: unknown) => error,
  );
}

beforeEach(() => {
  vi.stubGlobal("fetch", fetchMock);
  vi.stubGlobal("window", { location: { pathname: "/players/94589c87999b4a7ab3add2ae50711e64", search: "?tab=tags", assign } });
});

afterEach(() => {
  fetchMock.mockReset();
  vi.unstubAllGlobals();
});

describe("send", () => {
  it("posts JSON with the caller's signal and records Server-Timing for the query", async () => {
    fetchMock.mockResolvedValue(jsonResponse({ ok: true }, { headers: { "server-timing": "db;dur=3, app;dur=12.5" } }));
    const signal = new AbortController().signal;
    const timingKey = ["search", "query", "Kestrel_"];

    await expect(postJson("/search", { q: "Kestrel_" }, { signal, timingKey })).resolves.toEqual({ ok: true });

    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe("/api/search");
    expect(init).toMatchObject({ method: "POST", signal, body: JSON.stringify({ q: "Kestrel_" }) });
    expect(timingOf(hashKey(timingKey))).toMatchObject({ endpoint: "POST /api/search", status: 200, serverMs: 12.5 });
  });

  it("returns undefined for 204 No Content", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 204 }));
    await expect(getJson("/flags")).resolves.toBeUndefined();
  });

  it("turns the JSON error body into an ApiError with timing and server time", async () => {
    const date = "Wed, 14 Oct 2026 14:32:10 GMT";
    fetchMock.mockResolvedValue(
      jsonResponse({ error: { code: "timeout", message: "the query ran past 5 s" } }, { status: 504, headers: { "server-timing": "app;dur=5001", date } }),
    );
    const error = await failureOf(getJson("/requests/x"));
    expect(error).toBeInstanceOf(ApiError);
    expect(error).toMatchObject({ status: 504, code: "timeout", message: "the query ran past 5 s", endpoint: "GET /api/requests/x", serverMs: 5001, serverTime: date, retryable: false });
  });

  it("keeps a non-JSON error body as the message", async () => {
    fetchMock.mockResolvedValue(new Response("upstream reset", { status: 502 }));
    expect(await failureOf(getJson("/traffic/summary"))).toMatchObject({ status: 502, code: "http_502", message: "upstream reset", retryable: true });
  });

  it("reports an unreachable server as a retryable network error", async () => {
    fetchMock.mockRejectedValue(new TypeError("Failed to fetch"));
    expect(await failureOf(getJson("/session"))).toMatchObject({ status: 0, code: "network", message: "Could not reach the admin server: Failed to fetch", retryable: true });
  });

  it("rethrows an abort as is instead of reporting a network error", async () => {
    const controller = new AbortController();
    controller.abort();
    const abort = new DOMException("The operation was aborted.", "AbortError");
    fetchMock.mockRejectedValue(abort);
    expect(await failureOf(getJson("/session", { signal: controller.signal }))).toBe(abort);
  });

  it("never retries a client error", async () => {
    fetchMock.mockResolvedValue(jsonResponse({ error: { code: "not_found", message: "no such player" } }, { status: 404 }));
    expect(await failureOf(getJson("/players/x"))).toMatchObject({ code: "not_found", retryable: false });
  });

  it("sends the browser to login once, keeping the path and search", async () => {
    fetchMock.mockImplementation(async () => jsonResponse({ error: { code: "unauthorized", message: "sign in" } }, { status: 401 }));
    expect(await failureOf(getJson("/session"))).toMatchObject({ status: 401 });
    expect(await failureOf(getJson("/labels"))).toMatchObject({ status: 401 });
    expect(assign).toHaveBeenCalledTimes(1);
    expect(assign).toHaveBeenCalledWith(`/login?next=${encodeURIComponent("/players/94589c87999b4a7ab3add2ae50711e64?tab=tags")}`);
  });
});
