import type { ExplorerFilter } from "./explorer-filter";

type LoaderAnchor = { filter: string; at: number };

let loaderAnchor: LoaderAnchor | null = null;

export function rememberLoaderAnchor(filter: ExplorerFilter, at: number): void {
  loaderAnchor = { filter: JSON.stringify(filter), at };
}

export function loaderAnchorFor(filter: ExplorerFilter): number | null {
  return loaderAnchor?.filter === JSON.stringify(filter) ? loaderAnchor.at : null;
}
