import { NextResponse } from "next/server";
import { resolve } from "@/lib/api/coral";

function siteRedirect(path: string, request: Request): NextResponse {
  const origin = request.headers.get("x-forwarded-host")
    ? `${request.headers.get("x-forwarded-proto") || "https"}://${request.headers.get("x-forwarded-host")}`
    : process.env.SITE_URL || new URL(request.url).origin;
  return NextResponse.redirect(new URL(path, origin));
}

export async function GET(request: Request) {
  const query = new URL(request.url).searchParams.get("query")?.trim();
  if (!query) return siteRedirect("/?e=inv", request);

  const player = await resolve(query);
  if (!player) return siteRedirect("/?e=inv", request);

  return siteRedirect(`/player/${player.uuid}`, request);
}
