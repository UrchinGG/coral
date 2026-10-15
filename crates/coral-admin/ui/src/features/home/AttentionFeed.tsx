import { useQuery } from "@tanstack/react-query";
import { ArrowRight, TriangleAlert } from "lucide-react";
import { Link } from "react-router-dom";

import { flagsQuery, useDismissFlag } from "../../api/flags";
import type { Flag, FlagFailure, FlagKind } from "../../api/types/flags";
import type { FlagLevel } from "../../api/types/extension";
import { Button } from "../../kit/Button";
import { EntityLink } from "../../kit/EntityLink";
import { QueryContent } from "../../kit/QueryContent";
import { Section } from "../../kit/Section";
import { SkeletonRows } from "../../kit/States";
import { cx } from "../../lib/cx";
import { refHref, refIdentity } from "../../lib/refs";

const LEVEL_STRIPE: Record<FlagLevel, string> = { info: "bg-info", warning: "bg-warning", danger: "bg-danger" };
const ACCOUNT_TAB = "activity";
const UPSTREAM_TRAFFIC = "/traffic?window=1h";

const KIND_LABEL: Record<FlagKind, string> = {
  upstream: "upstream",
  writer: "writer",
  pressure: "limit pressure",
  probe: "auth probing",
  spike: "traffic spike",
  extension: "extension",
};

export function AttentionFeed() {
  const flags = useQuery(flagsQuery());
  return (
    <Section title="Attention" icon={TriangleAlert} count={flags.data?.flags.length} flush fetching={flags.isFetching && flags.data !== undefined}>
      <QueryContent
        query={flags}
        skeleton={<SkeletonRows count={3} />}
        isEmpty={(data) => data.flags.length === 0 && data.failures.length === 0}
        empty="Nothing needs attention. Dismissed flags come back after 24 hours if they still fire."
      >
        {(data) => (
          <ul className="m-0 flex list-none flex-col p-0">
            {data.failures.map((failure) => (
              <FailureRow key={failure.kind} failure={failure} />
            ))}
            {data.flags.map((flag) => (
              <FlagRow key={flag.key} flag={flag} />
            ))}
          </ul>
        )}
      </QueryContent>
    </Section>
  );
}

function FlagRow({ flag }: { flag: Flag }) {
  const dismiss = useDismissFlag();
  const target = investigateTarget(flag);
  return (
    <li className="grid grid-cols-[3px_minmax(0,1fr)_auto] items-start gap-3 border-b border-border px-3.5 py-2.5 last:border-b-0 max-md:grid-cols-[3px_minmax(0,1fr)]">
      <span aria-hidden className={cx("h-full min-h-9 rounded-sm", LEVEL_STRIPE[flag.level])} />
      <div className="flex min-w-0 flex-col gap-1">
        <div className="flex flex-wrap gap-2 text-11 font-semibold tracking-[0.05em] text-subtle uppercase">
          <span>{KIND_LABEL[flag.kind]}</span>
          <span>{flag.level}</span>
        </div>
        <div className="text-13 wrap-anywhere">{flag.summary}</div>
        {flag.refs.length > 0 && (
          <div className="flex flex-wrap gap-2.5 text-12">
            {flag.refs.map((ref) => (
              <EntityLink key={refIdentity(ref)} entity={ref} />
            ))}
          </div>
        )}
      </div>
      <div className="flex flex-wrap items-center gap-1.5 max-md:col-start-2">
        {target !== null && (
          <Link to={target} className="inline-flex h-6 items-center gap-1 rounded-md border border-border-strong bg-surface-2 px-2 text-11 font-medium hover:bg-overlay">
            Investigate <ArrowRight className="size-3" />
          </Link>
        )}
        <Button size="sm" variant="ghost" onClick={() => dismiss.mutate(flag.key)} disabled={dismiss.isPending}>
          Dismiss 24h
        </Button>
      </div>
    </li>
  );
}

function FailureRow({ failure }: { failure: FlagFailure }) {
  return (
    <li role="alert" className="grid grid-cols-[3px_minmax(0,1fr)] gap-3 border-b border-border px-3.5 py-2.5 last:border-b-0">
      <span aria-hidden className="h-full min-h-9 rounded-sm bg-danger" />
      <div className="flex min-w-0 flex-col gap-1">
        <div className="text-11 font-semibold tracking-[0.05em] text-danger uppercase">{KIND_LABEL[failure.kind]} check failed</div>
        <div className="font-mono text-12 wrap-anywhere text-muted">{failure.message}</div>
      </div>
    </li>
  );
}

function investigateTarget(flag: Flag): string | null {
  if (flag.kind === "extension") return flag.extension === null ? null : `/x/${encodeURIComponent(flag.extension)}`;
  if (flag.kind === "upstream") return UPSTREAM_TRAFFIC;
  const [first] = flag.refs;
  if (first === undefined) return null;
  const href = refHref(first);
  if (href === null) return null;
  return first.kind === "account" ? `${href}?${new URLSearchParams({ tab: ACCOUNT_TAB })}` : href;
}
