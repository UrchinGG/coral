import type { Camera } from "../viewer/pkg/replay_viewer.js";
import { CAMERAS } from "./cameras";

const params = new URLSearchParams(location.search);

export const requested = {
  src: params.get("src"),
  time: params.has("time") ? Number(params.get("time")) || 0 : undefined,
  camera: CAMERAS.find(({ id }) => id === params.get("camera"))?.id,
  player: params.has("player") ? Number(params.get("player")) : undefined,
  chat: params.get("chat") === "1",
  autoplay: params.get("autoplay") === "1",
};

const pageUrl = () => new URL(location.pathname, location.href);

export const keepSource = (src?: string) => {
  const url = pageUrl();
  if (src) url.searchParams.set("src", src);
  history.replaceState(null, "", url);
};

export const momentLink = (
  src: string,
  time: number,
  camera: Camera,
  player?: number,
) => {
  const url = pageUrl();
  url.searchParams.set("src", src);
  url.searchParams.set("time", String(Math.round(time / 100) * 100));
  if (camera !== "map") url.searchParams.set("camera", camera);
  if (player !== undefined) url.searchParams.set("player", String(player));
  return url.href;
};
