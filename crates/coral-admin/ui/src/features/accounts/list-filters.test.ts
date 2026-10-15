import { describe, expect, it } from "vitest";

import { listFiltersFrom } from "./list-filters";

describe("listFiltersFrom", () => {
  it("reads every filter the list page writes and ignores unknown values", () => {
    const filters = listFiltersFrom(new URLSearchParams({ sort: "requests", dir: "asc", rank: "3", locked: "no", has_dev_key: "yes", starfish: "maybe" }));
    expect(filters).toEqual({ sort: "requests", dir: "asc", rank: 3, locked: false, has_key: null, has_dev_key: true, starfish: null });
  });

  it("falls back to the newest members with no filter", () => {
    expect(listFiltersFrom(new URLSearchParams())).toEqual({ sort: "member", dir: "desc", rank: null, locked: null, has_key: null, has_dev_key: null, starfish: null });
  });
});
