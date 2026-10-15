import { hashKey, type QueryKey } from "@tanstack/react-query";

import { parseServerTiming, recordTiming } from "./timing";

export type ApiErrorInit = {
  status: number;
  code: string;
  message: string;
  endpoint: string;
  serverMs: number | null;
  serverTime: string | null;
};

export type RequestOptions = {
  signal?: AbortSignal;
  timingKey?: QueryKey;
};

type Method = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";

const API_PREFIX = "/api";
const LOGIN_PATH = "/login";
const JSON_CONTENT_TYPE = "application/json";
const NETWORK_STATUS = 0;
const UNAUTHORIZED = 401;
const NO_CONTENT = 204;
const SERVER_ERROR_FLOOR = 500;
const GATEWAY_TIMEOUT = 504;

let redirectingToLogin = false;

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly endpoint: string;
  readonly serverMs: number | null;
  readonly serverTime: string | null;

  constructor(init: ApiErrorInit) {
    super(init.message);
    this.name = "ApiError";
    this.status = init.status;
    this.code = init.code;
    this.endpoint = init.endpoint;
    this.serverMs = init.serverMs;
    this.serverTime = init.serverTime;
  }

  get retryable(): boolean {
    if (this.status === GATEWAY_TIMEOUT) return false;
    return this.status === NETWORK_STATUS || this.status >= SERVER_ERROR_FLOOR;
  }
}

export function isApiError(error: unknown): error is ApiError {
  return error instanceof ApiError;
}

export function getJson<T>(path: string, options: RequestOptions = {}): Promise<T> {
  return send<T>("GET", path, undefined, options);
}

export function postJson<T>(path: string, body: unknown, options: RequestOptions = {}): Promise<T> {
  return send<T>("POST", path, body, options);
}

export function putJson<T>(path: string, body: unknown, options: RequestOptions = {}): Promise<T> {
  return send<T>("PUT", path, body, options);
}

export function patchJson<T>(path: string, body: unknown, options: RequestOptions = {}): Promise<T> {
  return send<T>("PATCH", path, body, options);
}

export function deleteJson<T>(path: string, options: RequestOptions = {}): Promise<T> {
  return send<T>("DELETE", path, undefined, options);
}

async function send<T>(method: Method, path: string, body: unknown, options: RequestOptions): Promise<T> {
  const endpoint = `${method} ${API_PREFIX}${path}`;
  const startedAt = performance.now();
  const response = await fetchOrFail(endpoint, `${API_PREFIX}${path}`, {
    method,
    signal: options.signal,
    headers: body === undefined ? { Accept: JSON_CONTENT_TYPE } : { Accept: JSON_CONTENT_TYPE, "Content-Type": JSON_CONTENT_TYPE },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const serverMs = parseServerTiming(response.headers.get("server-timing"));
  if (options.timingKey !== undefined) {
    recordTiming(hashKey(options.timingKey), {
      endpoint,
      status: response.status,
      serverMs,
      clientMs: performance.now() - startedAt,
      at: Date.now(),
    });
  }
  if (response.status === UNAUTHORIZED) redirectToLogin();
  if (!response.ok) throw await errorFrom(response, endpoint, serverMs);
  if (response.status === NO_CONTENT) return undefined as T;
  return (await response.json()) as T;
}

async function fetchOrFail(endpoint: string, url: string, init: RequestInit): Promise<Response> {
  try {
    return await fetch(url, init);
  } catch (error) {
    if (init.signal?.aborted) throw error;
    throw new ApiError({
      status: NETWORK_STATUS,
      code: "network",
      message: `Could not reach the admin server: ${error instanceof Error ? error.message : String(error)}`,
      endpoint,
      serverMs: null,
      serverTime: null,
    });
  }
}

async function errorFrom(response: Response, endpoint: string, serverMs: number | null): Promise<ApiError> {
  const text = await response.text();
  const parsed = parseErrorBody(text);
  return new ApiError({
    status: response.status,
    code: parsed?.code ?? `http_${response.status}`,
    message: parsed?.message ?? (text.trim() || response.statusText || `HTTP ${response.status}`),
    endpoint,
    serverMs,
    serverTime: response.headers.get("date"),
  });
}

function parseErrorBody(text: string): { code: string; message: string } | null {
  const body = parseJsonObject(text);
  if (typeof body !== "object" || body === null || !("error" in body)) return null;
  const error: unknown = body.error;
  if (typeof error !== "object" || error === null) return null;
  const code = "code" in error && typeof error.code === "string" ? error.code : null;
  const message = "message" in error && typeof error.message === "string" ? error.message : null;
  return code === null || message === null ? null : { code, message };
}

function parseJsonObject(text: string): unknown {
  if (!text.trimStart().startsWith("{")) return null;
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

function redirectToLogin(): void {
  if (redirectingToLogin) return;
  redirectingToLogin = true;
  const next = window.location.pathname + window.location.search;
  window.location.assign(`${LOGIN_PATH}?next=${encodeURIComponent(next)}`);
}
