import { NextResponse } from "next/server";
import { starfishOrigin } from "@/lib/starfish/origin";

const DISCORD_CLIENT_ID = process.env.STARFISH_DISCORD_CLIENT_ID || "";
const DISCORD_CLIENT_SECRET = process.env.STARFISH_DISCORD_CLIENT_SECRET || "";

export async function GET(request: Request) {
  const origin = starfishOrigin(request);
  if (!origin) return NextResponse.json({ error: "Unrecognized host" }, { status: 400 });

  const code = new URL(request.url).searchParams.get("code");
  if (!code) return NextResponse.redirect(`${origin}/?error=no_code`);

  const tokenRes = await fetch("https://discord.com/api/v10/oauth2/token", {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({
      client_id: DISCORD_CLIENT_ID,
      client_secret: DISCORD_CLIENT_SECRET,
      grant_type: "authorization_code",
      code,
      redirect_uri: `${origin}/api/starfish/callback`,
    }),
  });

  if (!tokenRes.ok) return NextResponse.redirect(`${origin}/?error=auth_failed`);

  const { access_token } = (await tokenRes.json()) as { access_token: string };

  const response = NextResponse.redirect(`${origin}/`);
  response.cookies.set("sf_token", access_token, {
    httpOnly: true,
    secure: true,
    sameSite: "lax",
    maxAge: 3600,
    path: "/",
  });

  return response;
}
