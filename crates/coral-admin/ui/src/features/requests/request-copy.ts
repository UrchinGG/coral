import type { RequestFull } from "../../api/types/request-full";
import { buildCurl, type CurlCommand } from "../../lib/shell";

const CURL_COMPUTED_HEADERS = new Set(["content-length"]);
const JSON_INDENT = 2;

export function curlFor(full: Pick<RequestFull, "url" | "headers" | "body" | "request">): CurlCommand {
  const headers = Object.entries(full.headers ?? {}).filter(([name]) => !CURL_COMPUTED_HEADERS.has(name.toLowerCase()));
  return buildCurl({ method: full.request.method, url: full.url, headers, body: full.body });
}

export function requestJson(full: RequestFull): string {
  const { labels: _labels, ...request } = full;
  return JSON.stringify(request, null, JSON_INDENT);
}

export function storedBytes(body: string): number {
  return new TextEncoder().encode(body).length;
}
