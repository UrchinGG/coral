import type { ReactNode } from "react";
import type { RequestRow } from "../api/types";
import { fmtDate, fmtMs } from "../format";
import { Identity } from "./Identity";

function prettyJson(s: string): string {
  try {
    return JSON.stringify(JSON.parse(s), null, 2);
  } catch {
    return s;
  }
}

export function RequestModal({ request, onClose }: { request: RequestRow; onClose: () => void }) {
  const url = (request.path ?? "") + (request.query ? `?${request.query}` : "");
  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/60 p-8"
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <div className="w-full max-w-2xl rounded-lg border border-white/10 bg-[#12141c] p-5">
        <div className="mb-4 flex items-start justify-between gap-4">
          <div className="font-mono text-sm break-all">
            <span className="mr-2 rounded bg-white/10 px-1.5 py-0.5 text-xs">{request.method}</span>
            {url}
          </div>
          <button onClick={onClose} className="text-gray-400 hover:text-white">
            ✕
          </button>
        </div>
        <div className="grid grid-cols-2 gap-3 text-sm">
          <Field label="Time" value={fmtDate(request.ts)} />
          <Field label="Status" value={String(request.status ?? "—")} />
          <Field label="Latency" value={fmtMs(request.latency_ms)} />
          <Field label="Key" value={request.key_prefix ?? "—"} mono />
          <Field
            label="Account"
            value={<Identity id={request.uuid} username={request.minecraft_username} kind="minecraft" />}
          />
          <Field
            label="Discord"
            value={<Identity id={request.discord_id} username={request.discord_username} />}
          />
          <Field label="IP" value={request.ip ?? "—"} mono />
          <Field label="User-Agent" value={request.user_agent ?? "—"} />
        </div>
        {request.error && (
          <div className="mt-4">
            <div className="mb-1 text-xs text-gray-400">Error response</div>
            <pre className="max-h-64 overflow-auto rounded bg-black/40 p-3 text-xs">
              {prettyJson(request.error)}
            </pre>
          </div>
        )}
      </div>
    </div>
  );
}

function Field({ label, value, mono }: { label: string; value: ReactNode; mono?: boolean }) {
  return (
    <div>
      <div className="text-xs text-gray-500">{label}</div>
      <div className={mono ? "font-mono" : ""}>{value}</div>
    </div>
  );
}
