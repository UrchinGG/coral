import type { Metadata } from "next";
import { StarfishNav, StarfishFooter } from "@/components/starfish/Layout";
import { ReleaseNotes } from "@/components/starfish/ReleaseNotes";

export const metadata: Metadata = {
  title: "Patch Notes",
  description: "Patch notes and changelogs for Starfish.",
};

const API_URL = process.env.CORAL_API_URL || "http://localhost:8000";

type ReleaseInfo = {
  version: string;
  published_at: string;
  release_notes: string | null;
};

async function fetchReleases(): Promise<ReleaseInfo[]> {
  const res = await fetch(`${API_URL}/api/v1/starfish/download/releases`, { next: { revalidate: 300 } }).catch(() => null);
  if (!res?.ok) return [];
  return res.json();
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString("en-US", { year: "numeric", month: "long", day: "numeric" });
}

export default async function ReleasesPage() {
  const releases = await fetchReleases();

  return (
    <div className="min-h-screen flex flex-col">
      <StarfishNav active="releases" />

      <section className="flex-1 pt-28 px-6 pb-20">
        <div className="max-w-2xl mx-auto">
          <h1 className="text-3xl font-bold tracking-tight mb-12">Patch Notes</h1>

          {releases.length === 0 && (
            <p className="text-sm text-white/40">No releases available yet.</p>
          )}

          <div className="relative border-l border-white/[0.08] space-y-10">
            {releases.map((release) => (
              <ReleaseEntry key={release.version} release={release} />
            ))}
          </div>
        </div>
      </section>

      <StarfishFooter />
    </div>
  );
}

function ReleaseEntry({ release }: { release: ReleaseInfo }) {
  return (
    <div className="pl-8">
      <div className="mb-2">
        <span className="text-[11px] text-white/25">{formatDate(release.published_at)}</span>
      </div>

      {release.release_notes && <ReleaseNotes>{release.release_notes}</ReleaseNotes>}
    </div>
  );
}
