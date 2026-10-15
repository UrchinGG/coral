import type { ViewTone } from "../../api/types/extension";
import type { Tone } from "../../kit/tone";

export const VIEW_TONE: Record<ViewTone, Tone> = {
  neutral: "neutral",
  subtle: "neutral",
  ok: "ok",
  info: "info",
  warning: "warning",
  limited: "limited",
  danger: "danger",
};
