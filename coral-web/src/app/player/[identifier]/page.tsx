import { Metadata } from "next";
import { notFound, redirect, permanentRedirect } from "next/navigation";
import { resolve, getPlayer } from "@/lib/api/coral";
import { isUuidLike } from "@/lib/utils/general/validate";
import { getRank, getPlusColor, getDisplayName } from "@/lib/utils/hypixel/player/rank";
import { PlayerNav } from "@/components/PlayerNav";
import { PlayerHeader } from "@/components/PlayerHeader";
import { GeneralPanel } from "@/components/panels/player/GeneralPanel";
import { BedwarsPanel, DuelsPanel, SkywarsPanel, PitPanel, QuakePanel } from "@/components/panels";

export async function generateMetadata({ params }: { params: Promise<{ identifier: string }> }): Promise<Metadata> {
  const { identifier } = await params;
  const player = await resolve(identifier);
  const name = player?.username || identifier;
  const title = `${name}'s Stats`;
  const desc = `${name}'s Hypixel stats on Urchin.`;
  return {
    title, description: desc,
    alternates: { canonical: `/player/${encodeURIComponent(identifier)}` },
    openGraph: {
      title, description: desc,
      images: [{ url: `/api/og/player?name=${encodeURIComponent(name)}`, width: 1200, height: 630 }],
    },
    twitter: {
      card: "summary_large_image", title, description: desc,
      images: [`/api/og/player?name=${encodeURIComponent(name)}`],
    },
  };
}

export default async function PlayerPage({ params }: { params: Promise<{ identifier: string }> }) {
  const { identifier } = await params;
  const isUuid = isUuidLike(identifier);

  const resolved = await resolve(identifier);
  if (!resolved && !isUuid) notFound();
  const uuid = resolved?.uuid || identifier;

  if (!isUuid && resolved?.uuid) permanentRedirect(`/player/${resolved.uuid}`);

  const stats = await getPlayer(uuid);
  if (!stats?.hypixel) redirect("/?e=np");

  return (
    <div className="min-h-screen">
      <PlayerNav />
      <div className="max-w-7xl mx-auto px-4 pt-20 pb-10">
        <PlayerHeader hypixel={stats.hypixel} username={resolved?.username || identifier} uuid={uuid} />
        <div className="mt-8 grid grid-cols-1 lg:grid-cols-10 gap-6">
          <div className="lg:col-span-3">
            <GeneralPanel hypixel={stats.hypixel} />
          </div>
          <div className="lg:col-span-7 space-y-4">
            <BedwarsPanel hypixel={stats.hypixel} />
            <DuelsPanel hypixel={stats.hypixel} />
            <SkywarsPanel hypixel={stats.hypixel} />
            <PitPanel hypixel={stats.hypixel} />
            <QuakePanel hypixel={stats.hypixel} />
          </div>
        </div>
      </div>
    </div>
  );
}
