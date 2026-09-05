import { NextRequest, NextResponse } from "next/server";

const STARFISH_HOSTS = ["starfish.urchin.gg", "starfish.hexze.dev", "localhost:3000"];

export function proxy(request: NextRequest) {
  const host = request.headers.get("host") ?? "";
  if (!STARFISH_HOSTS.includes(host)) return NextResponse.next();

  const { pathname } = request.nextUrl;
  const rewritten = request.nextUrl.clone();
  rewritten.pathname = `/starfish${pathname === "/" ? "" : pathname}`;

  const headers = new Headers(request.headers);
  headers.set("x-starfish-rewrite", "1");
  return NextResponse.rewrite(rewritten, { request: { headers } });
}

export const config = {
  matcher: ["/((?!api|_next|.*\\..*).*)"],
};
