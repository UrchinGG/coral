import type { GuideContent } from "../../api/types/platform";

type GuideDraft = { base: string; content: GuideContent };

const GUIDE_DRAFT_KEY = "coral.guide-draft.v1";

export function draftOver(saved: GuideContent): GuideContent | null {
  const draft = readDraft();
  return draft !== null && draft.base === saved.body ? draft.content : null;
}

export function keepDraft(saved: GuideContent, edited: GuideContent): void {
  if (edited.body === saved.body) {
    discardDraft();
    return;
  }
  writeDraft({ base: saved.body, content: edited });
}

export function discardDraft(): void {
  try {
    window.sessionStorage.removeItem(GUIDE_DRAFT_KEY);
  } catch (error) {
    console.warn("Could not discard the guide draft", error);
  }
}

function readDraft(): GuideDraft | null {
  try {
    const raw = window.sessionStorage.getItem(GUIDE_DRAFT_KEY);
    return raw === null ? null : (JSON.parse(raw) as GuideDraft);
  } catch (error) {
    console.warn("Could not read the guide draft", error);
    return null;
  }
}

function writeDraft(draft: GuideDraft): void {
  try {
    window.sessionStorage.setItem(GUIDE_DRAFT_KEY, JSON.stringify(draft));
  } catch (error) {
    console.warn("Could not keep the guide draft", error);
  }
}
