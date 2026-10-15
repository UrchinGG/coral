import { describe, expect, it } from "vitest";

import type { SyncJob } from "../../api/types/platform";
import { membersPhrase, nicknamesPhrase, startedText } from "./labels";

const JOB: SyncJob = {
  id: 7,
  kind: "resync",
  label: "Applying display name format",
  state: "queued",
  processed: 0,
  total: 0,
  error: null,
  started_at: "2026-10-15T01:00:00.000000Z",
  finished_at: null,
};

describe("server labels", () => {
  it("names the job a change started", () => {
    expect(startedText({ job: JOB }, "Display name format saved")).toBe("Display name format saved; started “Applying display name format”");
    expect(startedText({ job: null }, "Linked role updated")).toBe("Linked role updated");
  });

  it("counts members only once the population is known", () => {
    expect(membersPhrase(4120)).toBe("4,120 members");
    expect(membersPhrase(undefined)).toBe("every member");
    expect(nicknamesPhrase(4120)).toBe("4,120 nicknames");
  });
});
