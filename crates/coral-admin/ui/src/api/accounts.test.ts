import { describe, expect, it } from "vitest";

import { accountListParams } from "./accounts";

describe("accountListParams", () => {
  it("sends only the filters that are set, plus sort, direction and cursor", () => {
    const params = accountListParams({ sort: "requests", dir: "asc", rank: 3, locked: false, has_key: null, has_dev_key: true, starfish: null }, "requests.10.4");
    expect(Object.fromEntries(params)).toEqual({ sort: "requests", dir: "asc", rank: "3", locked: "false", has_dev_key: "true", cursor: "requests.10.4" });
  });
});
