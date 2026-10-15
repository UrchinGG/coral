export const loadOmniboxPanel = () => import("./OmniboxPanel").then((module) => ({ default: module.OmniboxPanel }));

export const loadPalette = () => import("./Palette").then((module) => ({ default: module.Palette }));

export function prefetchOmnibox(): void {
  void loadOmniboxPanel();
  void loadPalette();
}
