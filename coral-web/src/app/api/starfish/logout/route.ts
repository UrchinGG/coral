import { NextResponse } from "next/server";
import { starfishOrigin } from "@/lib/starfish/origin";

export async function GET(request: Request) {
  const origin = starfishOrigin(request) ?? "https://starfish.urchin.gg";
  const response = NextResponse.redirect(`${origin}/`);
  response.cookies.delete("sf_token");
  return response;
}
