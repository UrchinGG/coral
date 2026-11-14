import { useQuery } from "@tanstack/react-query";
import { useState, type FormEvent } from "react";

import { keys } from "../../api/keys";
import { useAction } from "../../api/mutate";
import { addSyncSource, removeSyncSource, SERVER_INVALIDATES, setUsedSources, syncSourcesQuery } from "../../api/server";
import type { NewSyncSource, SyncSource } from "../../api/types/platform";
import { Button } from "../../kit/Button";
import { ConfirmButton } from "../../kit/ConfirmButton";
import { Input } from "../../kit/Input";
import { QueryContent } from "../../kit/QueryContent";
import { Section } from "../../kit/Section";
import { EmptyState } from "../../kit/States";
import { membersPhrase, startedText } from "./labels";

const SOURCES_INVALIDATE = [keys.server.sources()] as const;

type DataSourcesProps = { usedIds: string[]; memberCount: number | undefined };

export function DataSources({ usedIds, memberCount }: DataSourcesProps) {
  const sources = useQuery(syncSourcesQuery());
  return (
    <>
      <QueryContent query={sources}>{(list) => <SourceList key={usedIds.join(",")} sources={list} usedIds={usedIds} memberCount={memberCount} />}</QueryContent>
      <AddSource />
    </>
  );
}

function SourceList({ sources, usedIds, memberCount }: { sources: SyncSource[]; usedIds: string[]; memberCount: number | undefined }) {
  const [chosen, setChosen] = useState(() => new Set(usedIds));
  const dirty = chosen.size !== usedIds.length || usedIds.some((id) => !chosen.has(id));
  const save = useAction({
    run: () => setUsedSources([...chosen]),
    pending: () => "Saving the server's data sources…",
    done: (started) => startedText(started, "Data sources updated"),
    invalidates: SERVER_INVALIDATES,
  });
  const toggle = (id: string, on: boolean) => {
    const next = new Set(chosen);
    if (on) next.add(id);
    else next.delete(id);
    setChosen(next);
  };

  return (
    <Section
      title="Data sources"
      count={sources.length}
      actions={
        dirty && (
          <>
            <Button variant="primary" pending={save.isPending} onClick={() => save.mutate(undefined)}>
              Apply to {membersPhrase(memberCount)}
            </Button>
            <Button variant="ghost" onClick={() => setChosen(new Set(usedIds))}>
              Reset
            </Button>
          </>
        )
      }
    >
      {sources.length === 0 ? (
        <EmptyState>No data sources yet. Templates and autoroles read a source's fields as <span className="font-mono">name.field</span>.</EmptyState>
      ) : (
        <div className="flex flex-col divide-y divide-border">
          {sources.map((source) => (
            <SourceRow key={source.id} source={source} used={chosen.has(source.id)} onToggle={(on) => toggle(source.id, on)} />
          ))}
        </div>
      )}
    </Section>
  );
}

function SourceRow({ source, used, onToggle }: { source: SyncSource; used: boolean; onToggle: (on: boolean) => void }) {
  const remove = useAction({
    run: () => removeSyncSource(source.id),
    pending: () => `Removing ${source.name}…`,
    done: () => `Removed ${source.name}`,
    invalidates: [...SOURCES_INVALIDATE, ...SERVER_INVALIDATES],
  });
  return (
    <div className="flex flex-wrap items-start gap-4 py-3 first:pt-0 last:pb-0">
      <label className="flex min-w-0 flex-[1_1_16rem] items-start gap-2">
        <input type="checkbox" checked={used} onChange={(event) => onToggle(event.target.checked)} className="mt-0.5 size-3.5 accent-accent" />
        <span className="min-w-0">
          <span className="font-mono text-13 font-medium">{source.name}</span>
          <span className="mt-0.5 block truncate text-12 text-muted">{source.base_url}</span>
          <span className="mt-0.5 block text-12 text-muted">{freshness(source)}</span>
          {source.error !== null && <span className="mt-0.5 block text-12 text-danger">{source.error}</span>}
        </span>
      </label>
      <ConfirmButton variant="danger" confirmLabel={`Remove ${source.name}`} pending={remove.isPending} onConfirm={() => remove.mutate(undefined)}>
        Remove
      </ConfirmButton>
    </div>
  );
}

function freshness(source: SyncSource): string {
  const kept = `values kept for ${Math.round(source.max_age_secs / 60)} min`;
  switch (source.feed) {
    case "connected":
      return source.last_seq === null ? "Feed connected" : `Feed connected; last change #${source.last_seq}`;
    case "none":
      return `No change feed; ${kept}`;
    case "connecting":
      return "Connecting to the feed…";
    case "disconnected":
      return `Feed disconnected; ${kept} until it is back`;
  }
}

function AddSource() {
  const [name, setName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const add = useAction({
    run: (source: NewSyncSource) => addSyncSource(source),
    pending: (source) => `Adding ${source.name}…`,
    done: (_, source) => `Added ${source.name}`,
    invalidates: SOURCES_INVALIDATE,
    onDone: () => {
      setName("");
      setBaseUrl("");
      setApiKey("");
    },
  });
  const submit = (event: FormEvent) => {
    event.preventDefault();
    add.mutate({ name: name.trim(), base_url: baseUrl.trim(), api_key: apiKey === "" ? null : apiKey });
  };

  return (
    <Section title="Add a data source">
      <form onSubmit={submit} className="flex flex-wrap items-end gap-3">
        <Input label="Name" mono value={name} onChange={(event) => setName(event.target.value)} placeholder="rbw" className="w-40" />
        <Input label="Base URL" mono value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} placeholder="https://rbw.urchin.gg/coral/v1" className="w-80" />
        <Input label="Key" type="password" value={apiKey} onChange={(event) => setApiKey(event.target.value)} placeholder="optional" className="w-48" />
        <Button type="submit" variant="primary" pending={add.isPending} disabled={name.trim() === "" || baseUrl.trim() === ""}>
          Add source
        </Button>
      </form>
    </Section>
  );
}
