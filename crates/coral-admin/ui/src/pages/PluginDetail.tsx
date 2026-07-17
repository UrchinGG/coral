import { useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  useDeletePlugin,
  useDeleteRelease,
  usePlugin,
  useSetDisabled,
  useSetOfficial,
  useSetUnlisted,
  useUnyankRelease,
  useYankRelease,
} from "../api/plugins";
import type { Plugin, ReleaseView, ReviewView } from "../api/types";
import { Badge } from "../components/Badge";
import { ConfirmButton } from "../components/ConfirmButton";
import { Identity } from "../components/Identity";
import { fmtDate } from "../format";

export function PluginDetail() {
  const { slug } = useParams();
  const navigate = useNavigate();
  const plugin = usePlugin(slug ?? "");

  if (plugin.isLoading) {
    return <div className="text-sm text-gray-500">Loading…</div>;
  }
  if (!plugin.data) {
    return <div className="text-sm text-gray-500">Plugin not found.</div>;
  }

  const { plugin: p, releases, reviews, installs_30d, installs_total, owner_discord_id, owner_discord_username, owner_member_id } =
    plugin.data;

  return (
    <div className="flex flex-col gap-5">
      <button onClick={() => navigate("/plugins")} className="w-fit text-sm text-gray-400 hover:text-white">
        ← Back to plugins
      </button>

      <div className="rounded-lg border border-white/10 bg-white/5 p-4">
        <div className="flex flex-wrap items-center gap-3">
          <h1 className="text-lg font-semibold">{p.display_name}</h1>
          <span className="font-mono text-xs text-gray-500">{p.slug}</span>
          {p.official && <Badge label="Official" tone="ok" />}
          {p.unlisted && <Badge label="Unlisted" tone="warning" />}
          {p.disabled && <Badge label="Disabled" tone="danger" />}
        </div>
        <p className="mt-2 text-sm text-gray-400">{p.description}</p>
        <div className="mt-2 flex flex-wrap items-center gap-3 text-xs text-gray-500">
          <span>
            Owner: <Identity id={owner_discord_id} username={owner_discord_username} linkTo={owner_member_id ? `/members/${owner_member_id}` : undefined} />
          </span>
          <span>
            {installs_30d.toLocaleString()} installs (30d) / {installs_total.toLocaleString()} total
          </span>
          {p.homepage && (
            <a href={p.homepage} target="_blank" rel="noreferrer" className="text-gray-400 hover:text-white">
              Homepage ↗
            </a>
          )}
        </div>
        {p.disabled && p.disabled_reason && (
          <div className="mt-2 rounded border border-danger/30 bg-danger/10 p-2 text-xs text-danger">
            Disabled: {p.disabled_reason}
          </div>
        )}
      </div>

      <ActionBar plugin={p} onDeleted={() => navigate("/plugins")} />

      <ReleasesPanel slug={p.slug} releases={releases} />

      <ReviewsPanel reviews={reviews} />
    </div>
  );
}

function ActionBar({ plugin, onDeleted }: { plugin: Plugin; onDeleted: () => void }) {
  const setOfficial = useSetOfficial(plugin.slug);
  const setUnlisted = useSetUnlisted(plugin.slug);
  const setDisabled = useSetDisabled(plugin.slug);
  const deletePlugin = useDeletePlugin(plugin.slug);
  const [disableReason, setDisableReason] = useState("");

  return (
    <div className="flex flex-wrap items-center gap-2 rounded-lg border border-white/10 bg-white/5 p-4">
      <ConfirmButton
        label={plugin.official ? "Unset official" : "Mark official"}
        onConfirm={() => setOfficial.mutate(!plugin.official)}
        pending={setOfficial.isPending}
      />
      <ConfirmButton
        label={plugin.unlisted ? "Relist" : "Unlist"}
        tone={plugin.unlisted ? "default" : "danger"}
        onConfirm={() => setUnlisted.mutate(!plugin.unlisted)}
        pending={setUnlisted.isPending}
      />
      {plugin.disabled ? (
        <ConfirmButton
          label="Re-enable"
          onConfirm={() => setDisabled.mutate({ disabled: false })}
          pending={setDisabled.isPending}
        />
      ) : (
        <span className="flex items-center gap-1">
          <input
            className="w-48 rounded border border-white/10 bg-black/30 px-2 py-1 text-xs"
            placeholder="Disable reason…"
            value={disableReason}
            onChange={(e) => setDisableReason(e.target.value)}
          />
          <ConfirmButton
            label="Disable"
            tone="danger"
            disabled={!disableReason.trim()}
            onConfirm={() => setDisabled.mutate({ disabled: true, reason: disableReason })}
            pending={setDisabled.isPending}
          />
        </span>
      )}
      <ConfirmButton
        label="Delete plugin"
        tone="danger"
        onConfirm={() => deletePlugin.mutate(undefined, { onSuccess: onDeleted })}
        pending={deletePlugin.isPending}
      />
    </div>
  );
}

function ReleasesPanel({ slug, releases }: { slug: string; releases: ReleaseView[] }) {
  const yank = useYankRelease(slug);
  const unyank = useUnyankRelease(slug);
  const deleteRelease = useDeleteRelease(slug);

  return (
    <div className="rounded-lg border border-white/10 bg-white/5 p-4">
      <div className="mb-2 text-sm font-medium text-gray-300">Releases ({releases.length})</div>
      {releases.length === 0 ? (
        <div className="text-sm text-gray-500">No releases.</div>
      ) : (
        <div className="flex flex-col gap-2">
          {releases.map((r) => (
            <div key={r.id} className="rounded border border-white/5 p-2 text-sm">
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-medium">v{r.version}</span>
                {r.yanked && <Badge label="Yanked" tone="danger" />}
                <span className="text-xs text-gray-500">{fmtDate(r.created_at)}</span>
              </div>
              <div className="mt-1 font-mono text-[11px] text-gray-500" title={r.content_sha256 ?? undefined}>
                content: {r.content_sha256 ? `${r.content_sha256.slice(0, 16)}…` : "none"}
              </div>
              {r.yanked && r.yanked_reason && (
                <div className="mt-1 text-xs text-danger">Yanked: {r.yanked_reason}</div>
              )}
              <div className="mt-2 flex gap-2">
                {r.yanked ? (
                  <ConfirmButton
                    label="Unyank"
                    onConfirm={() => unyank.mutate(r.version)}
                    pending={unyank.isPending}
                  />
                ) : (
                  <ConfirmButton
                    label="Yank"
                    tone="danger"
                    onConfirm={() => yank.mutate({ version: r.version })}
                    pending={yank.isPending}
                  />
                )}
                <ConfirmButton
                  label="Delete release"
                  tone="danger"
                  onConfirm={() => deleteRelease.mutate(r.version)}
                  pending={deleteRelease.isPending}
                />
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function ReviewsPanel({ reviews }: { reviews: ReviewView[] }) {
  if (reviews.length === 0) return null;
  return (
    <div className="rounded-lg border border-white/10 bg-white/5 p-4">
      <div className="mb-2 text-sm font-medium text-gray-300">Reviews ({reviews.length})</div>
      <div className="flex flex-col gap-2">
        {reviews.map((r, i) => (
          <div key={i} className="rounded border border-white/5 p-2 text-sm">
            <div className="flex items-center gap-2">
              <Identity id={r.discord_id} username={r.discord_username} />
              <span className="text-warning">{"★".repeat(r.stars)}</span>
              <span className="text-xs text-gray-500">{fmtDate(r.updated_at)}</span>
            </div>
            {r.review && <div className="mt-1 text-xs text-gray-400">{r.review}</div>}
          </div>
        ))}
      </div>
    </div>
  );
}
