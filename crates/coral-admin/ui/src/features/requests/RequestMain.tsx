import { AlertTriangle } from "lucide-react";
import { useState } from "react";

import type { KeyDetail, QueryParam, RequestFull } from "../../api/types/request-full";
import { CopyButton } from "../../kit/Copy";
import { EntityLink } from "../../kit/EntityLink";
import { LazyJsonView } from "../../kit/LazyJsonView";
import { Section } from "../../kit/Section";
import { Segmented } from "../../kit/Segmented";
import { CoverageBanner, EmptyState } from "../../kit/States";
import { Table, type Column } from "../../kit/Table";
import { formatBytes, formatNumber } from "../../lib/format";
import { groupHeaders, HEADER_GROUP_LABEL, rawHeaders } from "./headers";
import { storedBytes } from "./request-copy";

type HeaderMode = "table" | "raw";

const HEADER_MODES: { value: HeaderMode; label: string }[] = [
  { value: "table", label: "Table" },
  { value: "raw", label: "Raw" },
];
const CODE_BLOCK = "m-0 max-h-[640px] overflow-auto px-3.5 py-3 font-mono text-12 break-all whitespace-pre-wrap";
const JSON_LINKS = { players: true } as const;
const SUCCESS_LIMIT = 400;

const PARAM_COLUMNS: readonly Column<QueryParam>[] = [
  { id: "name", header: "Name", cell: (param) => <span className="font-mono text-12">{param.name === "" ? "(empty)" : param.name}</span> },
  { id: "decoded", header: "Decoded", cell: (param) => <span className="font-mono text-12 break-all">{param.value}</span> },
  { id: "raw", header: "Raw", cell: (param) => <span className="font-mono text-12 break-all">{param.raw}</span>, priority: 2 },
  { id: "entity", header: "Entity", cell: (param) => (param.ref === null ? <span className="text-subtle">—</span> : <EntityLink entity={param.ref} />) },
];

export function RequestMain({ full }: { full: RequestFull }) {
  return (
    <div className="flex min-w-0 flex-col gap-4">
      {full.server_error !== null && <ServerError text={full.server_error} />}
      <QueryParams full={full} />
      {full.headers !== null && <Headers headers={full.headers} full={full} />}
      {full.detail_state === "present" && <Body full={full} />}
      <Response full={full} />
    </div>
  );
}

function ServerError({ text }: { text: string }) {
  return (
    <Section title="Server error" icon={AlertTriangle} count="internal detail the caller never saw" flush actions={<CopyButton text={text} label="Copy server error" />}>
      <pre className={`${CODE_BLOCK} text-danger`}>{text}</pre>
    </Section>
  );
}

function QueryParams({ full }: { full: RequestFull }) {
  const params = full.query_params;
  const pathSubjects = full.subjects.filter((subject) => subject.form === "path");
  return (
    <Section title="Query parameters" count={formatNumber(params.length)} flush>
      {params.length === 0 ? (
        <EmptyState>
          No query string.
          {pathSubjects.map((subject) => (
            <span key={subject.raw} className="ml-1.5">
              The path names <span className="font-mono">{subject.raw}</span>
              {subject.player !== null && (
                <>
                  {" "}
                  → <EntityLink entity={{ kind: "player", uuid: subject.player }} />
                </>
              )}
              .
            </span>
          ))}
        </EmptyState>
      ) : (
        <Table label="Query parameters" columns={PARAM_COLUMNS} rows={params} rowKey={(param) => `${param.name}=${param.raw}`} />
      )}
    </Section>
  );
}

function Headers({ headers, full }: { headers: Record<string, string>; full: RequestFull }) {
  const [mode, setMode] = useState<HeaderMode>("table");
  const count = Object.keys(headers).length;
  return (
    <Section title="Headers" count={formatNumber(count)} flush actions={<Segmented label="Header format" value={mode} options={HEADER_MODES} onChange={setMode} />}>
      {mode === "raw" ? <pre className={CODE_BLOCK}>{rawHeaders(headers)}</pre> : <HeaderTable headers={headers} full={full} />}
    </Section>
  );
}

function HeaderTable({ headers, full }: { headers: Record<string, string>; full: RequestFull }) {
  const key = full.caller.key;
  return (
    <div className="max-w-full overflow-x-auto">
      <table aria-label="Headers" className="w-full border-separate border-spacing-0 text-12">
        <tbody>
          {groupHeaders(headers).map(({ group, headers: entries }) => [
            <tr key={group}>
              <th colSpan={2} scope="colgroup" className="border-b border-border bg-surface-2 px-3.5 py-1 text-left text-11 font-semibold tracking-[0.05em] text-subtle uppercase">
                {HEADER_GROUP_LABEL[group]} · {entries.length}
              </th>
            </tr>,
            ...entries.map(([name, value]) => (
              <tr key={name}>
                <td className="w-[200px] border-b border-border px-3.5 py-1.5 align-top font-mono text-muted max-md:w-[120px]">{name}</td>
                <td className="border-b border-border px-3.5 py-1.5 align-top font-mono break-all">
                  <span className={group === "credentials" ? "text-entity-key" : undefined}>{value}</span>
                  {key !== null && key.key === value && <KeyOwner detail={key} />}
                </td>
              </tr>
            )),
          ])}
        </tbody>
      </table>
    </div>
  );
}

function KeyOwner({ detail }: { detail: KeyDetail }) {
  return (
    <span className="ml-2 font-sans text-muted">
      · {detail.owner === null ? "no matching key" : <EntityLink entity={detail.owner} />} · {detail.state}
    </span>
  );
}

function Body({ full }: { full: RequestFull }) {
  if (full.body === null) {
    return (
      <Section title="Body" flush>
        <EmptyState>No request body ({full.request.method}).</EmptyState>
      </Section>
    );
  }
  const stored = storedBytes(full.body);
  const received = full.body_bytes ?? stored;
  return (
    <Section
      title="Body"
      count={`${formatBytes(received)} · ${full.body_complete ? "complete" : "partly stored"}`}
      flush
      actions={<CopyButton text={full.body} label="Copy body" />}
    >
      {!full.body_complete && (
        <div className="px-3.5 pt-3">
          <CoverageBanner tone="warning">
            First {formatNumber(stored)} of {formatNumber(received)} bytes stored (request rejected; storage policy).
          </CoverageBanner>
        </div>
      )}
      {full.body_json !== null ? (
        <div className="px-3.5 py-3">
          <LazyJsonView value={full.body_json} raw={full.body} label="Request body" links={JSON_LINKS} />
        </div>
      ) : (
        <pre className={CODE_BLOCK}>{full.body}</pre>
      )}
    </Section>
  );
}

function Response({ full }: { full: RequestFull }) {
  const { request } = full;
  const outcome = request.outcome_status === null ? "" : ` → outcome ${request.outcome_status}`;
  return (
    <Section title="Response" count={`${request.status}${outcome}`} flush>
      {request.error !== null ? (
        <pre className={CODE_BLOCK}>{request.error}</pre>
      ) : (
        <EmptyState>{request.status < SUCCESS_LIMIT ? "Response bodies of successful requests are not stored." : "No response body was recorded."}</EmptyState>
      )}
    </Section>
  );
}
