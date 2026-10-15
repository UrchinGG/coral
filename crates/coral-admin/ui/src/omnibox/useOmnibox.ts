import { keepPreviousData, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";

import { prefetchEntity } from "../api/entities";
import { remoteSearchQuery, searchQuery } from "../api/search";
import type { EntityRef } from "../api/types/refs";
import type { LookupFailure, SearchHit } from "../api/types/search";
import { useDebouncedValue } from "../lib/debounce";
import { flushHistoryWrites } from "../lib/history-state";
import { classify, describeClassification, type Classification } from "./classify";
import { pushRecent, useRecent } from "./recent";
import {
  decisiveHits,
  enterTargetOf,
  hintItems,
  hitPath,
  mergeHits,
  recentItems,
  searchActions,
  serverItems,
  type OmniboxItem,
  type EnterTarget,
  type OmniboxSection,
  type PathTarget,
} from "./results";
import { omniboxStatus, type OmniboxStatus } from "./status";

export type PageJump = { label: string; path: string; detail: string };

export type OmniboxTarget = OmniboxItem | PathTarget;

export type OmniboxResults = {
  classification: Classification;
  hint: string;
  sections: OmniboxSection[];
  items: OmniboxItem[];
  status: OmniboxStatus;
  failures: readonly LookupFailure[];
  remoteError: Error | null;
  resolveEnter: () => Promise<EnterTarget | null>;
};

const SEARCH_DEBOUNCE_MS = 60;
const REMOTE_SETTLE_MS = 400;
const ENTER_SETTLE_MS = 400;
const NO_FAILURES: readonly LookupFailure[] = [];
const NO_HITS: readonly SearchHit[] = [];

export function useOmniboxResults(text: string, pages: readonly PageJump[]): OmniboxResults {
  const queryClient = useQueryClient();
  const recent = useRecent();
  const trimmed = text.trim();
  const classification = useMemo(() => classify(text, window.location.origin), [text]);
  const settled = useDebouncedValue(classification.text === "" ? "" : trimmed, SEARCH_DEBOUNCE_MS);
  const current = settled === trimmed && settled !== "";
  const search = useQuery({ ...searchQuery(settled), placeholderData: keepPreviousData });
  const settledData = current && !search.isPlaceholderData ? search.data : undefined;
  const remoteSettled = useDebouncedValue(classification.text, REMOTE_SETTLE_MS) === classification.text;
  const remote = useQuery(remoteSearchQuery(classification.text, remoteSettled && settledData?.remote_pending === true));
  const status = omniboxStatus({
    text: classification.text,
    current: current && (settledData !== undefined || search.error !== null),
    fetching: search.isFetching || remote.isFetching,
    error: search.error,
    count: search.data?.hits.length ?? 0,
  });
  const shownData = classification.text === "" ? undefined : search.data;
  const remoteHits = settledData === undefined ? NO_HITS : (remote.data ?? NO_HITS);
  const hits = useMemo(() => (shownData === undefined ? null : mergeHits(shownData.hits, remoteHits)), [shownData, remoteHits]);
  const failures = shownData?.failures ?? NO_FAILURES;
  const awaitingRemote = settledData?.remote_pending === true && remote.data === undefined;
  const sections = useMemo(() => sectionsOf(classification, hits, recent, pages), [classification, hits, pages, recent]);
  const items = useMemo(() => sections.flatMap((section) => section.items), [sections]);
  const latestQuery = useRef(trimmed);

  useEffect(() => {
    latestQuery.current = trimmed;
  }, [trimmed]);

  const resolveEnter = async (): Promise<EnterTarget | null> => {
    const settledHits = settledData === undefined ? await hitsSettledWithin(queryClient, trimmed, ENTER_SETTLE_MS) : decisiveHits(hits, failures, awaitingRemote);
    if (latestQuery.current !== trimmed) return null;
    return enterTargetOf(trimmed, classification, settledHits);
  };

  return { classification, hint: describeClassification(classification), sections, items, status, failures, remoteError: remote.error, resolveEnter };
}

export function useOmniboxNavigation(onDone: () => void) {
  const navigate = useNavigate();
  return (target: OmniboxTarget) => {
    flushHistoryWrites();
    onDone();
    if (target.type === "entity") {
      pushRecent(target.ref);
      const path = hitPath(target);
      if (path !== null) navigate(path);
      return;
    }
    navigate(target.path, { state: target.state ?? undefined });
  };
}

export function useOmniboxKeyboard(results: OmniboxResults, choose: (target: OmniboxTarget) => void) {
  const signature = results.items.map((item) => item.key).join("\n");
  const [selection, setSelection] = useState({ index: 0, moved: false, signature });
  const current = selection.signature === signature ? selection : { index: 0, moved: false, signature };
  if (current !== selection) setSelection(current);
  const active = Math.min(current.index, Math.max(results.items.length - 1, 0));
  const highlighted = results.items.at(active) ?? null;

  useEffect(() => {
    if (highlighted?.type === "entity") prefetchEntity(highlighted.ref);
  }, [highlighted]);

  const move = (index: number) => setSelection({ index, moved: true, signature });

  const onKeyDown = (event: KeyboardEvent) => {
    const count = results.items.length;
    if ((event.key === "ArrowDown" || event.key === "ArrowUp") && count > 0) {
      event.preventDefault();
      move((active + (event.key === "ArrowDown" ? 1 : -1) + count) % count);
      return;
    }
    if (event.key !== "Enter") return;
    event.preventDefault();
    if (current.moved || results.classification.text === "") {
      if (highlighted !== null) choose(highlighted);
      return;
    }
    void results.resolveEnter().then((target) => {
      if (target !== null) choose(target);
    });
  };

  return { active, move, onKeyDown };
}

function sectionsOf(classification: Classification, hits: readonly SearchHit[] | null, recent: readonly EntityRef[], pages: readonly PageJump[]): OmniboxSection[] {
  if (classification.text === "") {
    return [
      { title: "Recent", items: recentItems(recent) },
      { title: "Jump to", items: pages.map(pageItem) },
    ];
  }
  if (hits === null) {
    return [
      { title: "Suggestions", items: hintItems(classification) },
      { title: "Actions", items: searchActions(classification) },
    ];
  }
  return [
    { title: "Results", items: serverItems(hits, classification.operator) },
    { title: "Actions", items: searchActions(classification) },
  ];
}

function hitsSettledWithin(queryClient: QueryClient, query: string, waitMs: number): Promise<readonly SearchHit[] | null> {
  const outcome = queryClient.fetchQuery(searchQuery(query)).then(
    (response) => decisiveHits(response.hits, response.failures, response.remote_pending),
    () => null,
  );
  const deadline = new Promise<null>((resolve) => window.setTimeout(() => resolve(null), waitMs));
  return Promise.race([outcome, deadline]);
}

function pageItem(page: PageJump): OmniboxItem {
  return { type: "link", key: `page:${page.path}`, kind: "page", label: page.label, detail: page.detail, path: page.path, state: null };
}
