import { useCallback } from "react";

import type { PlayerForm, PlayerWindow, TopWindow } from "../../api/types/players";
import { normalizeUuid } from "../../lib/uuid";
import { useUrlEnum, useUrlParam } from "../../lib/url-state";

export const PLAYER_TABS = ["requests", "tags", "snapshots"] as const;
export const PLAYER_WINDOWS: readonly PlayerWindow[] = ["24h", "7d", "14d"];
export const DEFAULT_WINDOW: PlayerWindow = "14d";
export const FORM_ORDER: readonly PlayerForm[] = ["undashed", "dashed", "name", "former", "path", "body", "display"];
export const STANDARD_FORMS: readonly PlayerForm[] = FORM_ORDER.filter((form) => form !== "display");
export const TOP_WINDOWS: readonly TopWindow[] = ["1h", "24h"];
export const DEFAULT_TOP_WINDOW: TopWindow = "24h";

export type PlayerTab = (typeof PLAYER_TABS)[number];

const FORM_SEPARATOR = ",";

export function parseForms(raw: string | null): readonly PlayerForm[] {
  if (raw === null) return STANDARD_FORMS;
  const listed = new Set(raw.split(FORM_SEPARATOR));
  const forms = FORM_ORDER.filter((form) => listed.has(form));
  return forms.length === 0 ? STANDARD_FORMS : forms;
}

export function formsParam(forms: readonly PlayerForm[]): string | null {
  const ordered = FORM_ORDER.filter((form) => forms.includes(form));
  const standard = ordered.length === STANDARD_FORMS.length && STANDARD_FORMS.every((form) => ordered.includes(form));
  return standard ? null : ordered.join(FORM_SEPARATOR);
}

export function toggleForm(forms: readonly PlayerForm[], form: PlayerForm): readonly PlayerForm[] {
  if (!forms.includes(form)) return FORM_ORDER.filter((candidate) => candidate === form || forms.includes(candidate));
  if (forms.length === 1) return forms;
  return forms.filter((candidate) => candidate !== form);
}

export function formLabel(form: PlayerForm, currentName: string | null, formerNames: readonly string[]): string {
  if (form === "name" && currentName !== null) return `name "${currentName}"`;
  if (form === "former" && formerNames.length > 0) return `former ${formerNames.map((name) => `"${name}"`).join(", ")}`;
  return form;
}

export function parseWindow(raw: string | null): PlayerWindow {
  return PLAYER_WINDOWS.find((candidate) => candidate === raw) ?? DEFAULT_WINDOW;
}

export function parseTopWindow(raw: string | null): TopWindow {
  return TOP_WINDOWS.find((candidate) => candidate === raw) ?? DEFAULT_TOP_WINDOW;
}

export function canonicalPlayerLocation(raw: string, search: string): string | null {
  const uuid = normalizeUuid(raw);
  return uuid === null || uuid === raw ? null : `/players/${uuid}${search}`;
}

export function usePlayerTab() {
  return useUrlEnum<PlayerTab>("tab", PLAYER_TABS, "requests", { history: "push" });
}

export function useLookupSelection() {
  const [selectedWindow, setWindow] = useUrlEnum<PlayerWindow>("window", PLAYER_WINDOWS, DEFAULT_WINDOW);
  const [rawForms, setRawForms] = useUrlParam("forms");
  const forms = parseForms(rawForms);
  const toggle = useCallback((form: PlayerForm) => setRawForms(formsParam(toggleForm(forms, form))), [forms, setRawForms]);
  return { selection: { window: selectedWindow, forms }, setWindow, toggleForm: toggle };
}
