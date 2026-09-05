import { NextResponse } from "next/server";
import { starfishOrigin } from "@/lib/starfish/origin";

const DISCORD_CLIENT_ID = process.env.STARFISH_DISCORD_CLIENT_ID || "";

export async function GET(request: Request) {
  const origin = starfishOrigin(request);
  if (!origin) return NextResponse.json({ error: "Unrecognized host" }, { status: 400 });

  const params = new URLSearchParams({
    client_id: DISCORD_CLIENT_ID,
    redirect_uri: `${origin}/api/callback`,
    response_type: "code",
    scope: "identify",
  });

  return NextResponse.redirect(`https://discord.com/oauth2/authorize?${params}`);
}
