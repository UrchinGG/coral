import { useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { useAddTag, useLockPlayer, usePlayer, useRemoveTag, useUnlockPlayer } from "../api/players";
import { Badge } from "../components/Badge";
import { ConfirmButton } from "../components/ConfirmButton";
import { Identity } from "../components/Identity";
import { Panel } from "../components/Panel";
import { fmtDate } from "../format";

const TAG_TYPES = ["sniper", "blatant_cheater", "closet_cheater", "confirmed_cheater", "replays_needed", "caution"];

export function PlayerDetail() {
  const { uuid } = useParams();
  const navigate = useNavigate();
  const player = usePlayer(uuid ?? "");

  const addTag = useAddTag(uuid ?? "");
  const removeTag = useRemoveTag(uuid ?? "");
  const lockPlayer = useLockPlayer(uuid ?? "");
  const unlockPlayer = useUnlockPlayer(uuid ?? "");

  const [tagType, setTagType] = useState(TAG_TYPES[0]);
  const [reason, setReason] = useState("");
  const [lockReason, setLockReason] = useState("");

  if (player.isLoading) {
    return <div className="text-sm text-gray-500">Loading…</div>;
  }
  if (!player.data) {
    return <div className="text-sm text-gray-500">Player not found.</div>;
  }

  const { player: p, tags, tag_history } = player.data;

  return (
    <div className="flex flex-col gap-6">
      <button onClick={() => navigate("/players")} className="w-fit text-sm text-gray-400 hover:text-white">
        ← Back to players
      </button>

      <Panel>
        <div className="flex flex-wrap items-center gap-3">
          <Identity id={p.uuid} username={p.minecraft_username} kind="minecraft" />
          {p.is_locked && <Badge label="Locked" tone="danger" />}
        </div>
        {p.is_locked && (
          <div className="mt-2 text-xs text-gray-400">
            {p.lock_reason ?? "No reason given"} — locked by <Identity id={p.locked_by} username={p.locked_by_username} />{" "}
            on {fmtDate(p.locked_at)}
          </div>
        )}
        <div className="mt-3 flex flex-wrap items-center gap-2">
          {p.is_locked ? (
            <ConfirmButton label="Unlock" onConfirm={() => unlockPlayer.mutate()} pending={unlockPlayer.isPending} />
          ) : (
            <span className="flex items-center gap-1">
              <input
                className="w-48 rounded-md border border-white/10 bg-black/30 px-2 py-1 text-xs"
                placeholder="Lock reason (optional)…"
                value={lockReason}
                onChange={(e) => setLockReason(e.target.value)}
              />
              <ConfirmButton
                label="Lock player"
                tone="danger"
                onConfirm={() => lockPlayer.mutate(lockReason)}
                pending={lockPlayer.isPending}
              />
            </span>
          )}
        </div>
      </Panel>

      <Panel title={`Active tags (${tags.length})`}>
        <div className="flex flex-col divide-y divide-white/5">
          {tags.map((t) => (
            <div key={t.id} className="flex items-start justify-between gap-2 py-2 text-sm first:pt-0">
              <div>
                <Badge label={t.tag_type} />
                <div className="mt-1 text-xs text-gray-400">{t.reason}</div>
                <div className="text-xs text-gray-500">
                  added by <Identity id={t.added_by} username={t.added_by_username} /> on {fmtDate(t.added_on)}
                  {t.hide_username && " · username hidden"}
                </div>
              </div>
              <ConfirmButton
                label="Remove"
                tone="danger"
                onConfirm={() => removeTag.mutate(t.tag_type)}
                pending={removeTag.isPending}
              />
            </div>
          ))}
          {tags.length === 0 && <div className="text-sm text-gray-500">No active tags.</div>}
        </div>
        <div className="mt-3 flex flex-wrap gap-2">
          <select
            className="rounded-md border border-white/10 bg-black/30 px-2 py-1 text-xs"
            value={tagType}
            onChange={(e) => setTagType(e.target.value)}
          >
            {TAG_TYPES.map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
          </select>
          <input
            className="flex-1 rounded-md border border-white/10 bg-black/30 px-2 py-1 text-xs"
            placeholder="Reason…"
            value={reason}
            onChange={(e) => setReason(e.target.value)}
          />
          <ConfirmButton
            label="Add tag"
            disabled={!reason.trim()}
            onConfirm={() => addTag.mutate({ tag_type: tagType, reason, hide_username: false }, { onSuccess: () => setReason("") })}
            pending={addTag.isPending}
          />
        </div>
      </Panel>

      {tag_history.length > 0 && (
        <Panel title={`Tag history (${tag_history.length} removed)`}>
          <div className="flex flex-col divide-y divide-white/5">
            {tag_history.map((t) => (
              <div key={t.add_id} className="py-2 text-sm opacity-70 first:pt-0">
                <Badge label={t.tag_type} />
                <div className="mt-1 text-xs text-gray-400">{t.reason}</div>
                <div className="text-xs text-gray-500">
                  added by <Identity id={t.added_by} /> on {fmtDate(t.added_on)}
                  <br />
                  removed by <Identity id={t.removed_by} /> on {fmtDate(t.removed_on)}
                </div>
              </div>
            ))}
          </div>
        </Panel>
      )}
    </div>
  );
}
