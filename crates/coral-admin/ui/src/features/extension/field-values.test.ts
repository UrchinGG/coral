import { describe, expect, it } from "vitest";

import type { ViewField } from "../../api/types/extension";
import { initialValues, missingRequired } from "./field-values";

const FIELDS: ViewField[] = [
  { name: "name", label: "Name", required: true, kind: "text", placeholder: null, value: null },
  { name: "secret", label: "Secret", required: false, kind: "secret", placeholder: "paste it" },
  {
    name: "route",
    label: "Route",
    required: true,
    kind: "select",
    value: null,
    options: [
      { value: "route-1", label: "Route 1" },
      { value: "route-2", label: "Route 2" },
    ],
  },
];

describe("extension field values", () => {
  it("starts selects on their value or first option and never prefills secrets", () => {
    expect(initialValues(FIELDS)).toEqual({ name: "", secret: "", route: "route-1" });
  });

  it("blocks submission while a required field is blank", () => {
    expect(missingRequired(FIELDS, { name: "  ", route: "route-1" })).toBe(true);
    expect(missingRequired(FIELDS, { name: "edge", route: "route-1" })).toBe(false);
  });
});
