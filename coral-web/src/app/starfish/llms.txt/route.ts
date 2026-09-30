import { llmsTxt } from "@/lib/starfish/llms";
import { starfishOrigin } from "@/lib/starfish/origin";

export function GET(request: Request) {
  const origin = starfishOrigin(request);
  if (!origin || !request.headers.get("x-starfish-rewrite")) return new Response("Not Found", { status: 404 });

  return new Response(llmsTxt(origin), { headers: { "Content-Type": "text/plain; charset=utf-8" } });
}
