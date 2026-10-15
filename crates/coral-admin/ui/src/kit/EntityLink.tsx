import { useRef, type ReactNode } from "react";
import { Link } from "react-router-dom";

import { prefetchEntity } from "../api/entities";
import { requestLabel, useLabel } from "../api/labels";
import type { EntityRef } from "../api/types/refs";
import { cx } from "../lib/cx";
import { ENTITY_KIND_LABEL, labelTargetOf, refFallbackText, refHref, refId } from "../lib/refs";
import { useTimeFormat } from "../lib/time";
import { Badge } from "./Badges";
import { CopyButton } from "./Copy";
import { ENTITY_ICON, ENTITY_TEXT } from "./entity-style";
import { HoverCard } from "./HoverCard";

type EntityLinkProps = {
  entity: EntityRef;
  label?: string;
  mono?: boolean;
  eagerLabel?: boolean;
  tab?: string | null;
  plain?: boolean;
  className?: string;
};

const INTENT_DELAY_MS = 80;

export function EntityLink({ entity, label, mono, eagerLabel = true, tab = null, plain = false, className }: EntityLinkProps) {
  const target = labelTargetOf(entity);
  const name = useLabel(target, eagerLabel);
  const intentTimer = useRef<number | null>(null);
  const href = refHref(entity);
  const fallback = refFallbackText(entity);
  const text = label ?? name ?? fallback.text;
  const monospace = mono ?? (label === undefined && name === null && fallback.mono);

  if (href === null) return <UnlinkedEntity entity={entity} text={text} />;

  const requestOwnLabel = () => {
    if (target !== null) requestLabel(target);
  };
  const startIntent = () => {
    intentTimer.current = window.setTimeout(() => prefetchEntity(entity), INTENT_DELAY_MS);
  };
  const cancelIntent = () => {
    if (intentTimer.current !== null) window.clearTimeout(intentTimer.current);
    intentTimer.current = null;
  };
  const Icon = ENTITY_ICON[entity.kind];

  return (
    <HoverCard content={() => <EntityCard entity={entity} name={name} />} onOpen={requestOwnLabel}>
      <Link
        to={withTab(href, tab)}
        onPointerEnter={startIntent}
        onPointerLeave={cancelIntent}
        onFocus={() => {
          requestOwnLabel();
          prefetchEntity(entity);
        }}
        className={cx("group/el inline-flex max-w-full items-center gap-1 rounded-[3px] align-baseline font-medium", ENTITY_TEXT[entity.kind], className)}
      >
        {!plain && <Icon className="size-3 shrink-0 self-center opacity-85" />}
        <span className={cx("min-w-0 wrap-break-word group-hover/el:underline group-hover/el:decoration-1 group-hover/el:underline-offset-2", monospace && "font-mono text-12 font-normal")}>
          {text}
        </span>
        {entity.kind === "ip" && entity.proxied && <CfWorkerBadge />}
      </Link>
    </HoverCard>
  );
}

export function EntityLabel({ entity }: { entity: EntityRef }) {
  const name = useLabel(labelTargetOf(entity));
  const fallback = refFallbackText(entity);
  const Icon = ENTITY_ICON[entity.kind];
  return (
    <span className={cx("inline-flex max-w-full items-center gap-1 font-medium", ENTITY_TEXT[entity.kind])}>
      <Icon className="size-3 shrink-0 opacity-85" />
      <span className={cx("wrap-break-word", name === null && fallback.mono && "font-mono text-12 font-normal")}>{name ?? fallback.text}</span>
    </span>
  );
}

function UnlinkedEntity({ entity, text }: { entity: EntityRef; text: string }) {
  if (entity.kind === "internal") {
    const Icon = ENTITY_ICON.internal;
    return (
      <Badge variant="bot" icon={<Icon className="size-3" />}>
        {text}
      </Badge>
    );
  }
  return <span className="text-subtle">{text}</span>;
}

function CfWorkerBadge() {
  return (
    <Badge variant="cf" title="Arrived through a Cloudflare Worker; the IP shown is the Worker's outbound address">
      CF Worker
    </Badge>
  );
}

function EntityCard({ entity, name }: { entity: EntityRef; name: string | null }) {
  const id = refId(entity);
  const Icon = ENTITY_ICON[entity.kind];
  return (
    <div className="flex min-w-[220px] flex-col gap-2">
      <div className={cx("flex items-center gap-1.5 text-11 font-semibold tracking-[0.06em] uppercase", ENTITY_TEXT[entity.kind])}>
        <Icon className="size-3" />
        {ENTITY_KIND_LABEL[entity.kind]}
      </div>
      {name !== null && <div className="text-13 font-semibold text-fg">{name}</div>}
      <div className="flex items-start gap-1">
        <span className="min-w-0 font-mono text-12 break-all text-fg">{id}</span>
        <CopyButton text={id} label={`Copy ${ENTITY_KIND_LABEL[entity.kind].toLowerCase()} id`} />
      </div>
      <EntityFacts entity={entity} />
    </div>
  );
}

function EntityFacts({ entity }: { entity: EntityRef }) {
  const { time } = useTimeFormat();
  const facts: ReactNode[] = [];
  if (entity.kind === "account" && entity.member_id !== null) facts.push(`member #${entity.member_id}`);
  if (entity.kind === "request") facts.push(time(entity.ts, { millis: true, zone: true, date: "always" }));
  if (entity.kind === "ip") facts.push(entity.country ?? "country unknown", entity.proxied ? "via Cloudflare Worker" : "direct");
  if (entity.kind === "key") facts.push(`${entity.key_kind} key`, entity.key_id === null ? "prefix only" : `key #${entity.key_id}`);
  if (facts.length === 0) return null;
  return <div className="text-muted">{facts.join(" · ")}</div>;
}

function withTab(href: string, tab: string | null): string {
  if (tab === null) return href;
  return `${href}${href.includes("?") ? "&" : "?"}${new URLSearchParams({ tab })}`;
}
