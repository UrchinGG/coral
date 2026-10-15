import { useQuery } from "@tanstack/react-query";
import { createContext, use } from "react";

import { getJson } from "../api/http";
import { sessionQuery } from "../api/session";
import type { Boot } from "../api/types/session";

const BOOT_ELEMENT_ID = "boot";
const REQUIRED_OBJECTS = ["coverage"] as const;
const REQUIRED_STRINGS = ["discord_id", "now"] as const;
const REQUIRED_COVERAGE_OBJECTS = ["backfill", "writer"] as const;

export const BootContext = createContext<Boot | null>(null);

export async function loadBoot(): Promise<Boot> {
  const element = document.getElementById(BOOT_ELEMENT_ID);
  if (element === null) return parseBoot(await getJson<unknown>("/session"), "GET /api/session");
  return parseBoot(JSON.parse(element.textContent ?? ""), "the inlined boot payload");
}

export function useBoot(): Boot {
  const boot = use(BootContext);
  if (boot === null) throw new Error("useBoot needs a BootContext provider");
  return boot;
}

export function useSession(): Boot {
  return useQuery(sessionQuery(useBoot())).data;
}

export function parseBoot(value: unknown, source: string): Boot {
  const missing = missingFields(value);
  if (missing.length > 0) throw new Error(`${source} is missing ${missing.join(", ")}`);
  return value as Boot;
}

function missingFields(value: unknown): string[] {
  if (!isRecord(value)) return ["every field"];
  const missing: string[] = [];
  for (const field of REQUIRED_STRINGS) if (typeof value[field] !== "string") missing.push(field);
  for (const field of REQUIRED_OBJECTS) if (!isRecord(value[field])) missing.push(field);
  if (value.build !== null && typeof value.build !== "string") missing.push("build");
  if (!Array.isArray(value.extensions)) missing.push("extensions");
  const coverage = value.coverage;
  if (!isRecord(coverage)) return missing;
  if (!Array.isArray(coverage.gaps)) missing.push("coverage.gaps");
  for (const field of REQUIRED_COVERAGE_OBJECTS) if (!isRecord(coverage[field])) missing.push(`coverage.${field}`);
  return missing;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
