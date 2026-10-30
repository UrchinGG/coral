export const ASSETS = ["blocks", "items", "entities", "glyphs"].flatMap(
  (name) => [`${name}.bin`, `${name}.rgba`],
);

export const assetUrl = (name: string) => `/replay-assets/${name}.gz`;
