import type { Viewer } from "../viewer/pkg/replay_viewer.js";
import { h } from "./dom";

const root = document.documentElement.style;
let shown = new Map<string, string>();
let generation = 0;

const fetchSkin = async (hash: string) => {
  const response = await fetch(
    `https://textures.minecraft.net/texture/${hash}`,
  );
  if (!response.ok) throw new Error(`skin ${hash} answered ${response.status}`);
  const blob = await response.blob();
  const bitmap = await createImageBitmap(blob, {
    premultiplyAlpha: "none",
    colorSpaceConversion: "none",
  });
  const context = new OffscreenCanvas(bitmap.width, bitmap.height).getContext(
    "2d",
  )!;
  context.drawImage(bitmap, 0, 0);
  const pixels = context.getImageData(0, 0, bitmap.width, bitmap.height);
  return {
    rgba: new Uint8Array(pixels.data.buffer),
    url: URL.createObjectURL(blob),
  };
};

export const showSkins = (viewer: Viewer, hashes: string[]) => {
  const current = ++generation;
  for (const [hash, url] of shown) {
    root.removeProperty(`--skin-${hash}`);
    URL.revokeObjectURL(url);
  }
  shown = new Map();
  hashes.forEach(async (hash, index) => {
    try {
      const { rgba, url } = await fetchSkin(hash);
      if (current !== generation) return URL.revokeObjectURL(url);
      viewer.setSkin(index, rgba);
      shown.set(hash, url);
      root.setProperty(`--skin-${hash}`, `url(${url})`);
    } catch (error) {
      console.warn(error);
    }
  });
};

export const showFace = (face: HTMLElement, hash: string | null) =>
  hash
    ? face.style.setProperty("--face", `var(--skin-${hash})`)
    : face.style.removeProperty("--face");

export const face = (hash: string | null) => {
  const element = h("span", { className: "face" });
  showFace(element, hash);
  return element;
};
