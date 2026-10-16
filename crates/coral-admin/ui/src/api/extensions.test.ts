import { describe, expect, it } from "vitest";

import { extensionRefreshMs } from "./extensions";
import type { ExtensionView } from "./types/extension";

function view(refresh_s: number | null): ExtensionView {
  return { title: "Sample", subtitle: null, refresh_s, sections: [] };
}

describe("extensionRefreshMs", () => {
  it("polls at the interval the view asks for", () => {
    expect(extensionRefreshMs(view(5))).toBe(5_000);
  });

  it("stops polling when the view asks for no refresh", () => {
    expect(extensionRefreshMs(view(null))).toBe(false);
  });

  it("waits for the first view before polling", () => {
    expect(extensionRefreshMs(undefined)).toBe(false);
  });
});
