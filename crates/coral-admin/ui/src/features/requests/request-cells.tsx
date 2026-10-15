import { Fragment } from "react";

import type { Subject } from "../../api/types/refs";
import type { RequestRow } from "../../api/types/requests";
import { Badge } from "../../kit/Badges";
import { ClampText } from "../../kit/ClampText";
import { EntityLink } from "../../kit/EntityLink";
import { cx } from "../../lib/cx";
import { formatMs, formatNumber } from "../../lib/format";
import { FORM_LABEL, MATCH_LABEL, subjectRef } from "./subjects";

const SLOW_REQUEST_MS = 1_000;

export function SubjectChip({ subject }: { subject: Subject }) {
  const ref = subjectRef(subject);
  return (
    <span className="inline-flex max-w-full flex-wrap items-center gap-1 align-middle">
      {ref === null ? <span className="font-mono text-12 break-all">{subject.raw}</span> : <EntityLink entity={ref} label={ref.kind === "name" ? subject.raw : undefined} />}
      <Badge>{FORM_LABEL[subject.form]}</Badge>
    </span>
  );
}

export function SubjectsCell({ row }: { row: RequestRow }) {
  if (row.subjects.length === 0 && row.body_subjects === 0) return <span className="text-subtle">—</span>;
  return (
    <ClampText className="w-[260px]">
      {row.subjects.map((subject, index) => (
        <Fragment key={`${subject.form}:${subject.raw}:${index}`}>
          <SubjectChip subject={subject} />{" "}
        </Fragment>
      ))}
      {row.body_subjects > 0 && <Badge title="Players named in the request body; the full view lists them">+{formatNumber(row.body_subjects)} in body</Badge>}{" "}
      {row.match !== undefined && <Badge variant={row.match === "name_only" ? "byname" : "neutral"}>{MATCH_LABEL[row.match]}</Badge>}
    </ClampText>
  );
}

export function RequesterCell({ row }: { row: Pick<RequestRow, "requester" | "actor"> }) {
  return (
    <span className="inline-flex max-w-full flex-wrap items-center gap-x-1.5">
      <EntityLink entity={row.requester} />
      {row.actor !== null && (
        <>
          <span className="text-subtle">for</span>
          <EntityLink entity={row.actor} />
        </>
      )}
    </span>
  );
}

export function RouteCell({ row }: { row: Pick<RequestRow, "route" | "path"> }) {
  return (
    <span className="inline-flex max-w-full min-w-44 flex-wrap items-center gap-1.5">
      <span className="font-mono text-12 wrap-anywhere">{row.route ?? row.path}</span>
      {row.route === null && <Badge>unmatched</Badge>}
    </span>
  );
}

export function LatencyText({ ms }: { ms: number }) {
  return <span className={cx("font-mono", ms >= SLOW_REQUEST_MS && "text-warning")}>{formatMs(ms)}</span>;
}

export function IpCell({ row }: { row: Pick<RequestRow, "ip"> }) {
  return row.ip === null ? <span className="text-subtle">—</span> : <EntityLink entity={row.ip} />;
}

export function KeyCell({ row }: { row: Pick<RequestRow, "key_prefix" | "key_kind"> }) {
  if (row.key_prefix === null) return <span className="text-muted">{row.key_kind === "none" ? "no key presented" : row.key_kind}</span>;
  return (
    <span className="inline-flex flex-wrap items-center gap-1.5">
      <EntityLink entity={{ kind: "key", key_id: null, prefix: row.key_prefix, key_kind: row.key_kind }} />
      <span className="text-muted">{row.key_kind}</span>
    </span>
  );
}
