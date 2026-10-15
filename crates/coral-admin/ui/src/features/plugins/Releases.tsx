import { useState } from "react";

import { keys } from "../../api/keys";
import { errorText, useAction } from "../../api/mutate";
import { deleteRelease, unyankRelease, yankRelease } from "../../api/plugins";
import type { PluginRelease } from "../../api/types/platform";
import { Pill } from "../../kit/Badges";
import { Button } from "../../kit/Button";
import { ClampText } from "../../kit/ClampText";
import { ConfirmButton } from "../../kit/ConfirmButton";
import { ConfirmDialog, Dialog } from "../../kit/Dialog";
import { Input } from "../../kit/Input";
import { Section } from "../../kit/Section";
import { Table, type Column } from "../../kit/Table";
import { formatBytes } from "../../lib/format";
import { When } from "../platform/When";

type ReleasesProps = { slug: string; releases: readonly PluginRelease[]; fetching: boolean };

type ReleaseAction = { kind: "yank" | "delete"; release: PluginRelease } | null;

export function Releases({ slug, releases, fetching }: ReleasesProps) {
  const [action, setAction] = useState<ReleaseAction>(null);
  const invalidates = [keys.plugins.core(slug), keys.plugins.all()];
  const yank = useAction({
    run: ({ version, reason }: { version: string; reason: string }) => yankRelease(slug, version, reason),
    pending: ({ version }) => `Yanking ${version}…`,
    done: (_, { version }) => `${version} yanked`,
    invalidates,
  });
  const unyank = useAction({ run: (version: string) => unyankRelease(slug, version), pending: (version) => `Restoring ${version}…`, done: (_, version) => `${version} restored`, invalidates });
  const remove = useAction({ run: (version: string) => deleteRelease(slug, version), pending: (version) => `Deleting ${version}…`, done: (_, version) => `${version} deleted`, invalidates });

  const columns: readonly Column<PluginRelease>[] = [
    { id: "version", header: "Version", cell: (release) => <VersionCell release={release} /> },
    { id: "created", header: "Released", cell: (release) => <When at={release.created_at} /> },
    { id: "hashes", header: "Hashes", cell: (release) => <Hashes release={release} />, className: "min-w-[260px]", priority: 2 },
    { id: "size", header: "Size", cell: (release) => formatBytes(release.asset_size), numeric: true, priority: 2 },
    { id: "changelog", header: "Changelog", cell: (release) => (release.changelog === null ? <span className="text-subtle">—</span> : <ClampText>{release.changelog}</ClampText>), priority: 3 },
    {
      id: "actions",
      header: "",
      cell: (release) => (
        <span className="flex justify-end gap-1.5">
          {release.yanked ? (
            <ConfirmButton size="sm" confirmLabel="Restore" pending={unyank.isPending && unyank.variables === release.version} onConfirm={() => unyank.mutate(release.version)}>
              Unyank
            </ConfirmButton>
          ) : (
            <Button size="sm" variant="danger" onClick={() => setAction({ kind: "yank", release })}>
              Yank…
            </Button>
          )}
          <Button size="sm" variant="danger" onClick={() => setAction({ kind: "delete", release })}>
            Delete…
          </Button>
        </span>
      ),
    },
  ];

  return (
    <Section title="Releases" count={releases.length} flush fetching={fetching}>
      <Table label="Releases" columns={columns} rows={releases} rowKey={(release) => String(release.id)} empty="No release has been published." />
      <YankDialog release={action?.kind === "yank" ? action.release : null} onClose={() => setAction(null)} onYank={(version, reason) => yank.mutateAsync({ version, reason })} />
      <ConfirmDialog
        open={action?.kind === "delete"}
        onClose={() => setAction(null)}
        title={`Delete release ${action?.release.version ?? ""}?`}
        confirmLabel="Delete release"
        tone="danger"
        expect={action?.release.version}
        onConfirm={() => (action === null ? Promise.resolve() : remove.mutateAsync(action.release.version).then(() => undefined))}
      >
        <p className="m-0">The release and its stored asset are removed for good. Yank it instead to keep it restorable.</p>
      </ConfirmDialog>
    </Section>
  );
}

function VersionCell({ release }: { release: PluginRelease }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="flex items-center gap-1.5">
        <span className="font-mono text-12 font-medium">{release.version}</span>
        {release.yanked && <Pill tone="danger">yanked</Pill>}
      </span>
      {release.yanked && (
        <span className="text-11 text-muted">
          <When at={release.yanked_at} /> {release.yanked_reason ?? "no reason recorded"}
        </span>
      )}
    </div>
  );
}

function Hashes({ release }: { release: PluginRelease }) {
  return (
    <dl className="m-0 grid grid-cols-[auto_minmax(0,1fr)] gap-x-2 font-mono text-11">
      <dt className="text-subtle">commit</dt>
      <dd className="m-0 break-all">{release.git_sha}</dd>
      <dt className="text-subtle">asset</dt>
      <dd className="m-0 break-all">{release.asset_sha256}</dd>
      <dt className="text-subtle">content</dt>
      <dd className="m-0 break-all">{release.content_sha256 ?? "none"}</dd>
    </dl>
  );
}

function YankDialog({ release, onClose, onYank }: { release: PluginRelease | null; onClose: () => void; onYank: (version: string, reason: string) => Promise<unknown> }) {
  const [reason, setReason] = useState("");
  const [pending, setPending] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const close = () => {
    setReason("");
    setFailure(null);
    onClose();
  };
  const submit = () => {
    if (release === null) return;
    setPending(true);
    onYank(release.version, reason.trim())
      .then(close, (error: unknown) => setFailure(errorText(error)))
      .finally(() => setPending(false));
  };
  return (
    <Dialog
      open={release !== null}
      onClose={close}
      title={`Yank ${release?.version ?? ""}`}
      actions={
        <>
          <Button variant="ghost" onClick={close}>
            Cancel
          </Button>
          <Button variant="danger" pending={pending} onClick={submit}>
            Yank release
          </Button>
        </>
      }
    >
      <p className="m-0">A yanked release stays stored and can be restored, but clients are no longer offered it.</p>
      <Input label="Reason (optional)" value={reason} onChange={(event) => setReason(event.target.value)} error={failure} />
    </Dialog>
  );
}
